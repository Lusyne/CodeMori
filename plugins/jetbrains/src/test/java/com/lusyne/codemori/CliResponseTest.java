package com.lusyne.codemori;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;
import static org.junit.jupiter.api.Assertions.*;

class CliResponseTest {
    private static final String VALID = """
            {"protocol_version":1,"ok":true,"data":{"version":"0.1.0",
            "data_dir":"/tmp/中文 data","database_path":"/tmp/中文 data/codemori.sqlite3","schema_version":1}}
            """;

    @Test void readsActualBundledCore(@TempDir Path root) throws Exception {
        Path data = root.resolve("中文 data");
        Process process = new ProcessBuilder(System.getProperty("codemori.testCli"),
                "info", "--data-dir", data.toString()).start();
        try {
            assertTrue(process.waitFor(10, TimeUnit.SECONDS), "CLI timed out");
            assertEquals(0, process.exitValue());
            String json = new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
            assertEquals(data.toString(), CliResponse.parse(json).dataDirectory());
            assertFalse(Files.exists(data), "info must not initialize storage");
        } finally {
            process.destroyForcibly();
        }
    }

    @Test void rpcBridgeUsesActualCore(@TempDir Path root) throws Exception {
        CoreClient client = new CoreClient(Path.of(System.getProperty("codemori.testCli")), root.resolve("store"));
        var create = CoreClient.request("record_create");
        var record = new com.google.gson.JsonObject();
        record.addProperty("kind", "snippet");
        record.addProperty("title", "支付重试");
        record.addProperty("content", "retryPayment();");
        create.add("record", record);
        var saved = client.call(create, null).getAsJsonObject();
        assertEquals(1, saved.get("revision").getAsInt());
        var search = CoreClient.request("search");
        var filter = new com.google.gson.JsonObject();
        filter.addProperty("query", "重试");
        search.add("filter", filter);
        var results = client.call(search, null).getAsJsonObject();
        assertEquals(1, results.get("total").getAsInt());
        assertEquals(saved.get("id"), results.getAsJsonArray("items").get(0).getAsJsonObject().getAsJsonObject("record").get("id"));
    }

    @Test void sharesRecordsWithTheVsCodeClient(@TempDir Path root) throws Exception {
        String script = System.getProperty("codemori.vscodeInterop");
        org.junit.jupiter.api.Assumptions.assumeTrue(script != null, "Enable cross-client test with -PvscodeInterop=<compiled script>");
        Path binary = Path.of(System.getProperty("codemori.testCli"));
        CoreClient client = new CoreClient(binary, root.resolve("shared 中文"));
        var request = CoreClient.request("record_create");
        var input = new com.google.gson.JsonObject(); input.addProperty("kind", "snippet");
        input.addProperty("title", "来自 JetBrains"); input.addProperty("content", "sharedSnapshot();"); request.add("record", input);
        var saved = client.call(request, null).getAsJsonObject();
        Process node = new ProcessBuilder("node", script, binary.toString(), root.resolve("shared 中文").toString(), saved.get("id").getAsString()).start();
        try {
            assertTrue(node.waitFor(20, TimeUnit.SECONDS));
            assertEquals(0, node.exitValue(), new String(node.getErrorStream().readAllBytes(), StandardCharsets.UTF_8));
            var read = CoreClient.request("record_get"); read.add("id", saved.get("id"));
            var changed = client.call(read, null).getAsJsonObject();
            assertEquals(2, changed.get("revision").getAsLong());
            assertEquals("VS Code 更新", changed.getAsJsonObject("input").get("title").getAsString());
            assertTrue(changed.getAsJsonObject("input").get("starred").getAsBoolean());
            assertEquals("sharedSnapshot();", changed.getAsJsonObject("input").get("content").getAsString());
        } finally { node.destroyForcibly(); }
        var search = CoreClient.request("search"); var filter = new com.google.gson.JsonObject();
        filter.addProperty("query", "vs CODE 更新"); filter.addProperty("tag", "中文"); filter.addProperty("starred", true); search.add("filter", filter);
        assertEquals(1, client.call(search, null).getAsJsonObject().get("total").getAsInt());
        var read = CoreClient.request("record_get"); read.add("id", saved.get("id"));
        var changed = client.call(read, null).getAsJsonObject();
        var edited = changed.getAsJsonObject("input").deepCopy(); edited.addProperty("title", "JetBrains 再次更新"); edited.addProperty("content", "updatedSnapshot();");
        var tags = new com.google.gson.JsonArray(); tags.add("Redis"); tags.add("中文"); edited.add("tags", tags);
        var update = CoreClient.request("record_update"); update.add("id", saved.get("id")); update.add("revision", changed.get("revision")); update.add("record", edited);
        client.call(update, null);
        var fromNode = runNode(script, binary, root.resolve("shared 中文"), saved.get("id").getAsString(), "delete-and-create");
        var missing = assertThrows(Exception.class, () -> client.call(read, null)); assertTrue(missing.getMessage().contains("NOT_FOUND"));
        var getNode = CoreClient.request("record_get"); getNode.add("id", fromNode.get("id"));
        assertEquals("newFromVSCode();", client.call(getNode, null).getAsJsonObject().getAsJsonObject("input").get("content").getAsString());
        assertEquals("新建", client.call(CoreClient.request("tags"), null).getAsJsonArray().get(0).getAsString());
        var delete = CoreClient.request("record_delete"); delete.add("id", fromNode.get("id")); delete.add("revision", fromNode.get("revision")); client.call(delete, null);
        assertTrue(runNode(script, binary, root.resolve("shared 中文"), fromNode.get("id").getAsString(), "empty").get("empty").getAsBoolean());
    }

    private static com.google.gson.JsonObject runNode(String script, Path binary, Path store, String id, String phase) throws Exception {
        Process process = new ProcessBuilder("node", script, binary.toString(), store.toString(), id, phase).start();
        try {
            assertTrue(process.waitFor(20, TimeUnit.SECONDS), "Node interoperability test timed out");
            assertEquals(0, process.exitValue(), new String(process.getErrorStream().readAllBytes(), StandardCharsets.UTF_8));
            return com.google.gson.JsonParser.parseString(new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8)).getAsJsonObject();
        } finally { process.destroyForcibly(); }
    }

    @Test void decodesUnicodePaths() {
        CliResponse info = CliResponse.parse(VALID);
        assertEquals("/tmp/中文 data", info.dataDirectory());
        assertEquals("0.1.0", info.version());
    }

    @Test void rejectsFutureAndFractionalProtocols() {
        assertThrows(IllegalArgumentException.class, () -> CliResponse.parse(VALID.replace("protocol_version\":1", "protocol_version\":2")));
        assertThrows(IllegalArgumentException.class, () -> CliResponse.parse(VALID.replace("protocol_version\":1", "protocol_version\":1.5")));
    }

    @Test void rejectsMissingOrMistypedFields() {
        assertThrows(IllegalArgumentException.class, () -> CliResponse.parse("{}"));
        assertThrows(IllegalArgumentException.class, () -> CliResponse.parse(VALID.replace("\"ok\":true", "\"ok\":\"true\"")));
        assertThrows(IllegalArgumentException.class, () -> CliResponse.parse(VALID.replace("\"version\":\"0.1.0\"", "\"version\":null")));
    }

    @Test void propagatesCoreError() {
        var error = assertThrows(IllegalArgumentException.class, () -> CliResponse.parse("""
                {"protocol_version":1,"ok":false,"error":{"code":"HOME_UNAVAILABLE","message":"No home"}}
                """));
        assertEquals("HOME_UNAVAILABLE: No home", error.getMessage());
    }
}
