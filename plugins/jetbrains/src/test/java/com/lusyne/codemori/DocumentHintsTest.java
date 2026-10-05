package com.lusyne.codemori;
import com.google.gson.JsonObject;
import com.intellij.openapi.application.ReadAction;
import com.intellij.openapi.util.Disposer;
import com.intellij.openapi.vfs.LocalFileSystem;
import com.intellij.psi.PsiManager;
import com.intellij.testFramework.EdtTestUtil;
import com.intellij.testFramework.PlatformTestUtil;
import com.intellij.testFramework.fixtures.BasePlatformTestCase;
import java.nio.file.Files;
import java.nio.file.Path;
public final class DocumentHintsTest extends BasePlatformTestCase {
    private Path temp;
    @Override protected void tearDown() throws Exception {
        try { super.tearDown(); } finally { if(temp!=null)try(var paths=Files.walk(temp)){for(Path p:paths.sorted(java.util.Comparator.reverseOrder()).toList())Files.deleteIfExists(p);} }
    }
    public void testNativeHintToggleLeavesModuleAssociationIntactAndRefreshesOnSavedChanges() throws Exception {
        temp=Files.createTempDirectory("codemori-hints-").toRealPath();Path root=temp.resolve("repo");Files.createDirectories(root.resolve("pay"));Path file=root.resolve("pay/A.java");Files.writeString(file,"class A {}\n");
        com.intellij.openapi.vfs.newvfs.impl.VfsRootAccess.allowRootAccess(getTestRootDisposable(),temp.toString());
        CoreClient client=new CoreClient(Path.of(System.getProperty("codemori.testCli")),temp.resolve("data"));
        JsonObject register=CoreClient.request("workspace_register");register.addProperty("root",root.toString());String workspace=client.call(register,null).getAsJsonObject().get("id").getAsString();
        JsonObject create=CoreClient.request("record_create"),input=new JsonObject();input.addProperty("kind","document");input.addProperty("title","Module design");input.addProperty("url","https://example.com/hints");create.add("record",input);String id=client.call(create,null).getAsJsonObject().get("id").getAsString();
        JsonObject binding=new JsonObject();binding.addProperty("workspace_id",workspace);binding.addProperty("document_id",id);binding.addProperty("path","pay");binding.addProperty("kind","module");JsonObject link=CoreClient.request("document_link");link.add("binding",binding);client.call(link,null);
        DocumentHints hints=new DocumentHints(getProject(),root.toString(),client);Disposer.register(getTestRootDisposable(),hints);
        var vf=EdtTestUtil.runInEdtAndGet(()->LocalFileSystem.getInstance().refreshAndFindFileByNioFile(file));assertNotNull(vf);
        var psi=ReadAction.compute(()->PsiManager.getInstance(getProject()).findFile(vf));assertNotNull(psi);
        var leaf=ReadAction.compute(()->psi.findElementAt(0));DocumentMarker marker=new DocumentMarker(hints::summary);
        EdtTestUtil.runInEdtAndWait(()->PlatformTestUtil.waitWithEventsDispatching("No native document hint",()->ReadAction.compute(()->marker.getLineMarkerInfo(leaf))!=null,20));
        assertTrue(ReadAction.compute(()->marker.getLineMarkerInfo(leaf).getLineMarkerTooltip()).contains("1 篇关联文档"));
        EdtTestUtil.runInEdtAndWait(hints::toggle);assertNull(ReadAction.compute(()->marker.getLineMarkerInfo(leaf)));
        JsonObject get=CoreClient.request("file_documents");get.addProperty("workspace_id",workspace);get.addProperty("path","pay/A.java");assertEquals(1,client.call(get,null).getAsJsonArray().size());
        EdtTestUtil.runInEdtAndWait(hints::toggle);Files.writeString(file,"class A { int changed; }\n");EdtTestUtil.runInEdtAndWait(hints::invalidate);
        EdtTestUtil.runInEdtAndWait(()->PlatformTestUtil.waitWithEventsDispatching("No review hint",()->{String value=ReadAction.compute(()->hints.summary(psi));return value!=null&&value.contains("待复核");},20));
    }
}
