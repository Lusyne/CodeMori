package com.lusyne.codemori;

import com.intellij.openapi.progress.EmptyProgressIndicator;
import com.intellij.openapi.progress.ProcessCanceledException;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Timeout;
import org.junit.jupiter.api.condition.DisabledOnOs;
import org.junit.jupiter.api.condition.OS;
import org.junit.jupiter.api.io.TempDir;
import static org.junit.jupiter.api.Assertions.*;

/** Real Unix child processes and real deadlines; no timer mocks or IDE windows. */
@DisabledOnOs(OS.WINDOWS)
class CoreClientFailureTest {
    @BeforeAll
    static void initializePlatform() {
        com.intellij.testFramework.TestApplicationManager.getInstance();
    }

    private CoreClient client(Path root) throws IOException {
        Path executable = root.resolve("hanging-cli");
        Files.copy(Path.of(System.getProperty("codemori.hangingCliFixture")), executable);
        assertTrue(executable.toFile().setExecutable(true, true));
        return new CoreClient(executable, root);
    }

    private long childPid(Path root) throws Exception {
        Path file = root.resolve("child.pid");
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5);
        while (!Files.exists(file) && System.nanoTime() < deadline) Thread.sleep(10);
        return Long.parseLong(Files.readString(file).trim());
    }

    private void assertStopped(long pid) throws InterruptedException {
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(2);
        while (ProcessHandle.of(pid).map(ProcessHandle::isAlive).orElse(false) && System.nanoTime() < deadline) Thread.sleep(10);
        assertFalse(ProcessHandle.of(pid).map(ProcessHandle::isAlive).orElse(false), "RPC child remained alive after failure");
    }

    private void cleanup(Path root) throws Exception {
        if (Files.exists(root.resolve("child.pid"))) ProcessHandle.of(childPid(root)).filter(ProcessHandle::isAlive).ifPresent(ProcessHandle::destroyForcibly);
    }

    @Test @Timeout(45)
    void realDeadlineReportsTimeoutAndTerminatesChild(@TempDir Path root) throws Exception {
        CoreClient client = client(root);
        long started = System.nanoTime();
        try {
            IOException failure = assertThrows(IOException.class, () -> client.call(CoreClient.request("search"), null));
            assertTrue(failure.getMessage().contains("timed out"));
            long elapsed = TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - started);
            assertTrue(elapsed >= 29_000 && elapsed < 45_000, "Unexpected deadline: " + elapsed);
            assertStopped(childPid(root));
        } finally { cleanup(root); }
    }

    @Test @Timeout(10)
    void inFlightCancellationIsNotReportedAsSuccess(@TempDir Path root) throws Exception {
        CoreClient client = client(root);
        EmptyProgressIndicator indicator = new EmptyProgressIndicator();
        try (var executor = Executors.newSingleThreadExecutor()) {
            var future = executor.submit(() -> client.call(CoreClient.request("search"), indicator));
            try {
                long pid = childPid(root); indicator.cancel();
                var failure = assertThrows(java.util.concurrent.ExecutionException.class, () -> future.get(5, TimeUnit.SECONDS));
                assertInstanceOf(ProcessCanceledException.class, failure.getCause());
                assertStopped(pid);
            } finally { indicator.cancel(); cleanup(root); executor.shutdownNow(); }
        }
    }
}
