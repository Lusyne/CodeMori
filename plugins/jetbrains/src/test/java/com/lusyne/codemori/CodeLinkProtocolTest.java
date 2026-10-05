package com.lusyne.codemori;

import com.google.gson.JsonObject;
import com.intellij.openapi.fileEditor.FileEditorManager;
import com.intellij.openapi.extensions.ExtensionPointName;
import com.intellij.openapi.application.JBProtocolCommand;
import com.intellij.testFramework.EdtTestUtil;
import com.intellij.testFramework.PlatformTestUtil;
import com.intellij.testFramework.fixtures.BasePlatformTestCase;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Future;

public final class CodeLinkProtocolTest extends BasePlatformTestCase {
    private Path temp;
    @Override protected void tearDown() throws Exception {
        try { super.tearDown(); }
        finally { if (temp != null) try (var paths = Files.walk(temp)) { for (Path p : paths.sorted(java.util.Comparator.reverseOrder()).toList()) Files.deleteIfExists(p); } }
    }
    public void testRegisteredProtocolNavigatesIntoIndependentCloneAtEncodedPathAndLine() throws Exception {
        temp = Files.createTempDirectory("codemori-code-link-").toRealPath();
        Path origin = temp.resolve("origin"), clone = temp.resolve("clone");
        for (Path root : List.of(origin, clone)) { Files.createDirectories(root.resolve("src")); Files.writeString(root.resolve("src/中文 +#%.java"), "// header\nclass Example {}\n// footer\n"); }
        var client = new CoreClient(Path.of(System.getProperty("codemori.testCli")), temp.resolve("private"));
        JsonObject create = CoreClient.request("code_link_create"); create.addProperty("root", origin.toString()); create.addProperty("path", "src/中文 +#%.java"); create.addProperty("line", 2);
        JsonObject links = client.call(create, null).getAsJsonObject();
        Files.createDirectories(clone.resolve(".codemori")); Files.copy(origin.resolve(".codemori/project.json"), clone.resolve(".codemori/project.json"));
        com.intellij.openapi.vfs.newvfs.impl.VfsRootAccess.allowRootAccess(getTestRootDisposable(), temp.toString());
        var handler = new CodeLinkProtocol(() -> client, () -> List.of(new CodeLinks.ProjectRoot(getProject(), clone.toString())));
        Map<String,String> parameters=new java.util.HashMap<>();
        for(String pair:java.net.URI.create(links.get("jetbrains_url").getAsString()).getRawQuery().split("&")){String[] parts=pair.split("=",2);parameters.put(parts[0],java.net.URLDecoder.decode(parts[1],java.nio.charset.StandardCharsets.UTF_8));}
        Future<String> result = EdtTestUtil.runInEdtAndGet(() -> handler.perform("open", parameters, null));
        EdtTestUtil.runInEdtAndWait(() -> PlatformTestUtil.waitWithEventsDispatching("Code link did not complete", result::isDone, 20));
        assertNull(result.get());
        EdtTestUtil.runInEdtAndWait(() -> {
            var editor = FileEditorManager.getInstance(getProject()).getSelectedTextEditor(); assertNotNull(editor);
            assertEquals(1, editor.getCaretModel().getLogicalPosition().line);
            var file = com.intellij.openapi.fileEditor.FileDocumentManager.getInstance().getFile(editor.getDocument()); assertNotNull(file);
            assertTrue(Files.isSameFile(clone.resolve("src/中文 +#%.java"), file.toNioPath()));
        });
        EdtTestUtil.runInEdtAndWait(() -> {
            TestDialogs.set(com.intellij.openapi.ui.TestDialog.YES, getTestRootDisposable());
            CodeMoriPanel panel = new CodeMoriPanel(getProject(), client);
            com.intellij.openapi.util.Disposer.register(getTestRootDisposable(), panel);
            panel.copyCodeLink(new SelectionContext(clone.toString(), "src/中文 +#%.java", 2, ""));
            PlatformTestUtil.waitWithEventsDispatching("Copy code link did not reach clipboard", () -> {
                String text = com.intellij.openapi.ide.CopyPasteManager.getInstance().getContents(java.awt.datatransfer.DataFlavor.stringFlavor);
                return links.get("jetbrains_url").getAsString().equals(text);
            }, 20);
        });
        assertTrue(ExtensionPointName.<JBProtocolCommand>create("com.intellij.jbProtocolCommand").getExtensionList().stream().anyMatch(c -> c instanceof CodeLinkProtocol));
        assertNotNull(handler.perform("delete", Map.of(), null).get());
        assertNotNull(handler.perform("open", Map.of(), "bad").get());
    }
}
