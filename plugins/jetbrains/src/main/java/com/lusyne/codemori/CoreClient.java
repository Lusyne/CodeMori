package com.lusyne.codemori;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.intellij.execution.ExecutionException;
import com.intellij.execution.configurations.GeneralCommandLine;
import com.intellij.execution.process.CapturingProcessHandler;
import com.intellij.execution.process.ProcessOutput;
import com.intellij.openapi.application.PathManager;
import com.intellij.openapi.progress.ProcessCanceledException;
import com.intellij.openapi.progress.ProgressIndicator;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Locale;
import java.util.concurrent.atomic.AtomicReference;

/** Executes a single RPC on a background thread. Storage rules remain in Rust. */
public final class CoreClient {
    private final Path executable;
    private final Path dataDirectory;

    CoreClient(Path executable, Path dataDirectory) {
        this.executable = executable;
        this.dataDirectory = dataDirectory;
    }

    public static CoreClient bundled() {
        Path jar = PathManager.getJarForClass(CoreClient.class);
        if (jar == null || !Files.isRegularFile(jar) || jar.getParent() == null || !"lib".equals(jar.getParent().getFileName().toString())) {
            throw new IllegalArgumentException("CodeMori installation not found.");
        }
        Path pluginPath = jar.getParent().getParent();
        String osName = System.getProperty("os.name").toLowerCase(Locale.ROOT);
        String os = osName.contains("mac") ? "macos" : osName.contains("windows") ? "windows"
                : osName.contains("linux") ? "linux" : "unsupported";
        String arch = switch (System.getProperty("os.arch")) {
            case "aarch64", "arm64" -> "arm64";
            case "amd64", "x86_64" -> "x86_64";
            default -> "unsupported";
        };
        String file = os.equals("windows") ? "codemori.exe" : "codemori";
        Path executable = pluginPath.resolve("bin").resolve(os + "-" + arch).resolve(file);
        String testData = System.getProperty("codemori.testDataDir");
        return new CoreClient(executable, testData == null ? null : Path.of(testData));
    }

    public JsonElement call(JsonObject request, ProgressIndicator indicator) throws ExecutionException, IOException {
        if (!Files.isExecutable(executable)) {
            throw new IOException("Bundled CodeMori CLI is missing for this platform. Install the matching package.");
        }
        GeneralCommandLine command = new GeneralCommandLine(executable.toString(), "rpc").withCharset(StandardCharsets.UTF_8);
        if (dataDirectory != null) command.addParameters("--data-dir", dataDirectory.toString());
        JsonObject envelope = new JsonObject();
        envelope.addProperty("protocol_version", 1);
        envelope.add("request", request);
        byte[] input = envelope.toString().getBytes(StandardCharsets.UTF_8);
        CapturingProcessHandler handler = new CapturingProcessHandler(command);
        AtomicReference<IOException> writeError = new AtomicReference<>();
        Thread writer = Thread.ofVirtual().start(() -> {
            try (var stdin = handler.getProcess().getOutputStream()) {
                stdin.write(input);
            } catch (IOException error) {
                writeError.set(error);
            }
        });
        try {
            ProcessOutput output = indicator == null ? handler.runProcess(30_000)
                    : handler.runProcessWithProgressIndicator(indicator, 30_000, true);
            if (output.isCancelled()) throw new ProcessCanceledException();
            if (output.isTimeout()) throw new IOException("CodeMori request timed out. Refresh before retrying a change.");
            if (writeError.get() != null) throw writeError.get();
            JsonElement data = CliResponse.data(output.getStdout());
            if (output.getExitCode() != 0) throw new IOException("CodeMori exited with " + output.getExitCode() + ": " + output.getStderr());
            return data;
        } finally {
            if (!handler.isProcessTerminated()) handler.destroyProcess();
            try { writer.join(1000); }
            catch (InterruptedException error) { Thread.currentThread().interrupt(); }
        }
    }

    public static JsonObject request(String operation) {
        JsonObject request = new JsonObject();
        request.addProperty("op", operation);
        return request;
    }
}
