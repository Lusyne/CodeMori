import org.jetbrains.intellij.platform.gradle.tasks.PrepareSandboxTask
import org.jetbrains.intellij.platform.gradle.TestFrameworkType

plugins {
    java
    id("org.jetbrains.intellij.platform") version "2.19.0"
}

group = "com.lusyne"
version = "0.1.0"

repositories {
    mavenCentral()
    intellijPlatform { defaultRepositories() }
}

dependencies {
    intellijPlatform {
        val localIde = providers.gradleProperty("localIdePath")
        if (localIde.isPresent) local(localIde.get()) else intellijIdeaCommunity("2024.2")
        testFramework(TestFrameworkType.Platform)
    }
    testImplementation("org.junit.jupiter:junit-jupiter:5.11.4")
    testImplementation("junit:junit:4.13.2")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
    testRuntimeOnly("org.junit.vintage:junit-vintage-engine:5.11.4")
}

java { toolchain { languageVersion = JavaLanguageVersion.of(21) } }

intellijPlatform {
    pluginConfiguration {
        id = "com.lusyne.codemori"
        name = "CodeMori"
        version = project.version.toString()
        ideaVersion {
            sinceBuild = "242"
            // Explicitly unset: omission defaults to the SDK's major build range.
            untilBuild = provider { null }
        }
        vendor { name = "Lusyne" }
    }
    buildSearchableOptions = false
}

val workspaceRoot = layout.projectDirectory.dir("../..")
val os = System.getProperty("os.name").lowercase().let {
    when {
        it.contains("mac") -> "macos"
        it.contains("windows") -> "windows"
        it.contains("linux") -> "linux"
        else -> error("Unsupported build OS: $it")
    }
}
val arch = when (val value = System.getProperty("os.arch")) {
    "aarch64", "arm64" -> "arm64"
    "amd64", "x86_64" -> "x86_64"
    else -> error("Unsupported build architecture: $value")
}
val executable = if (os == "windows") "codemori.exe" else "codemori"
val nativeBinary = workspaceRoot.file("target/release/$executable")
val buildNativeCli by tasks.registering(Exec::class) {
    workingDir(workspaceRoot)
    environment("CARGO_TARGET_DIR", workspaceRoot.dir("target").asFile.absolutePath)
    commandLine("cargo", "build", "--locked", "--release", "-p", "codemori-cli")
    inputs.files(workspaceRoot.file("Cargo.toml"), workspaceRoot.file("Cargo.lock"))
    inputs.dir(workspaceRoot.dir("crates"))
    outputs.file(nativeBinary)
}

tasks.withType<PrepareSandboxTask>().configureEach {
    dependsOn(buildNativeCli)
    from(nativeBinary) {
        into("${pluginName.get()}/bin/$os-$arch")
        filePermissions { unix("rwxr-xr-x") }
    }
    from(workspaceRoot.file("LICENSE")) {
        into(pluginName.get())
    }
}

tasks.test {
    useJUnitPlatform()
    dependsOn(buildNativeCli)
    inputs.file(nativeBinary)
    systemProperty("codemori.testCli", nativeBinary.asFile.absolutePath)
    val hangingCli = workspaceRoot.file("tests/fixtures/hanging-cli.sh")
    inputs.file(hangingCli)
    systemProperty("codemori.hangingCliFixture", hangingCli.asFile.absolutePath)
    providers.gradleProperty("vscodeInterop").orNull?.let {
        systemProperty("codemori.vscodeInterop", it)
        inputs.dir(file(it).parentFile.parentFile)
    }
}
tasks.withType<JavaCompile>().configureEach { options.encoding = "UTF-8" }
// The foundation archive contains one host binary, not a universal distribution.
tasks.named<Zip>("buildPlugin") {
    archiveClassifier.set("$os-$arch")
    // Gradle 9 archives normalize file permissions; the native CLI must stay executable.
    filesMatching("**/bin/**") { permissions { unix("rwxr-xr-x") } }
}

// Explicit sandbox-only overrides keep interactive QA away from personal app data.
tasks.named<JavaExec>("runIde") {
    providers.gradleProperty("codemoriTestDataDir").orNull?.let { systemProperty("codemori.testDataDir", it) }
    providers.gradleProperty("codemoriTestProject").orNull?.let { args(it) }
    systemProperty("ide.show.tips.on.startup", "false")
}
