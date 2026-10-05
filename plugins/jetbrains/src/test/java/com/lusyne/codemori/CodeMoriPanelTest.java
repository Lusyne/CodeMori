package com.lusyne.codemori;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.application.ModalityState;
import com.intellij.openapi.ide.CopyPasteManager;
import com.intellij.openapi.ui.TestDialog;
import com.intellij.openapi.ui.TestDialogManager;
import com.intellij.openapi.util.Disposer;
import com.intellij.testFramework.EdtTestUtil;
import com.intellij.testFramework.PlatformTestUtil;
import com.intellij.testFramework.fixtures.BasePlatformTestCase;
import java.awt.Component;
import java.awt.Container;
import java.awt.datatransfer.DataFlavor;
import java.awt.image.BufferedImage;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import javax.imageio.ImageIO;
import javax.swing.*;

/** Component-level integration inside IntelliJ's test application, using the actual Rust executable. */
public final class CodeMoriPanelTest extends BasePlatformTestCase {
    private CoreClient client;
    private Path storage;
    @Override protected void setUp() throws Exception {
        super.setUp();
        storage = Files.createTempDirectory("codemori-ui-test-");
        client = new CoreClient(Path.of(System.getProperty("codemori.testCli")), storage.resolve("store"));
    }
    @Override protected void tearDown() throws Exception {
        try { super.tearDown(); }
        finally {
            if (storage != null) {
                try (var paths = Files.walk(storage)) {
                    for (Path path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) Files.deleteIfExists(path);
                }
            }
        }
    }
    private JsonElement call(JsonObject request) throws Exception {
        return CompletableFuture.supplyAsync(() -> {
            try { return client.call(request, null); }
            catch (Exception error) { throw new java.util.concurrent.CompletionException(error); }
        }).get(20, TimeUnit.SECONDS);
    }
    private JsonObject seed(String kind, String title, String content) throws Exception {
        JsonObject input = new JsonObject(); input.addProperty("kind", kind); input.addProperty("title", title);
        if (kind.equals("snippet")) { input.addProperty("content", content); input.addProperty("language", "java"); }
        else { input.addProperty("description", content); input.addProperty("url", "https://example.com/design"); }
        JsonObject request = CoreClient.request("record_create"); request.add("record", input);
        return call(request).getAsJsonObject();
    }
    private CodeMoriPanel panel() throws Exception {
        return EdtTestUtil.runInEdtAndGet(() -> {
            CodeMoriPanel panel = new CodeMoriPanel(getProject(), client);
            Disposer.register(getTestRootDisposable(), panel);
            return panel;
        });
    }
    private static <T extends Component> T named(Container root, String name, Class<T> type) {
        for (Component component : root.getComponents()) {
            if (name.equals(component.getName())) return type.cast(component);
            if (component instanceof Container container) {
                T result = named(container, name, type); if (result != null) return result;
            }
        }
        return null;
    }
    private static AbstractButton button(Container root, String text) {
        for (Component component : root.getComponents()) {
            if (component instanceof AbstractButton button && text.equals(button.getText())) return button;
            if (component instanceof Container container) { AbstractButton result = button(container, text); if (result != null) return result; }
        }
        return null;
    }
    private static <T> JLabel firstDocumentLabel(JList<T> list) { return (JLabel)list.getCellRenderer().getListCellRendererComponent(list,list.getModel().getElementAt(0),0,false,false); }
    private static void awaitStatus(CodeMoriPanel panel, String expected) throws Exception {
        EdtTestUtil.runInEdtAndWait(() -> PlatformTestUtil.waitWithEventsDispatching(
                () -> "Status: " + named(panel, "codemori.status", JTextArea.class).getText(),
                () -> named(panel, "codemori.status", JTextArea.class).getText().contains(expected), 15));
    }
    public void testResultLayoutIsReusedAcrossRowsAndReleasedOnRefresh() throws Exception {
        seed("snippet", "First result", "ordinary implementation line\n".repeat(150));
        seed("snippet", "Second result", "ordinary implementation line\n".repeat(150));
        CodeMoriPanel panel = panel(); awaitStatus(panel, "共 2 条");
        AtomicReference<Component> before = new AtomicReference<>();
        EdtTestUtil.runInEdtAndWait(() -> {
            JList<?> list = named(panel, "codemori.results", JList.class);
            Component first = renderRow(list, 0, false);
            Object layout = ((JComponent)first).getClientProperty(javax.swing.plaf.basic.BasicHTML.propertyKey);
            assertNotNull(layout);
            renderRow(list, 1, false);
            Component repeated = renderRow(list, 0, false);
            assertSame(first, repeated);
            assertSame("Repeated row layout must reuse parsed HTML", layout,
                    ((JComponent)repeated).getClientProperty(javax.swing.plaf.basic.BasicHTML.propertyKey));
            Component selected = renderRow(list, 0, true);
            assertEquals(list.getSelectionBackground(), selected.getBackground());
            before.set(first);
            JTextField query = named(panel, "codemori.search", JTextField.class);
            query.setText("ordinary"); query.postActionEvent();
        });
        awaitStatus(panel, "共 2 条");
        EdtTestUtil.runInEdtAndWait(() -> {
            JList<?> list = named(panel, "codemori.results", JList.class);
            Component after = renderRow(list, 0, false);
            assertNotSame("New results must not retain the old page layout", before.get(), after);
            assertTrue(((JLabel)after).getText().contains("<b><u>ordinary</u></b>"));
        });
    }
    private static <T> Component renderRow(JList<T> list, int index, boolean selected) {
        return list.getCellRenderer().getListCellRendererComponent(list, list.getModel().getElementAt(index), index, selected, false);
    }

    public void testSearchFavoriteCopyAndDeleteThroughThePanel() throws Exception {
        JsonObject snippet = seed("snippet", "支付重试", "retryPayment();");
        seed("document", "设计说明", "超时重试需要幂等键");
        CodeMoriPanel panel = panel(); awaitStatus(panel, "共 2 条");
        EdtTestUtil.runInEdtAndWait(() -> {
            JTextField query = named(panel, "codemori.search", JTextField.class);
            query.setText("支付"); query.postActionEvent();
        });
        awaitStatus(panel, "共 1 条");
        EdtTestUtil.runInEdtAndWait(() -> {
            assertTrue(named(panel, "codemori.preview", JTextArea.class).getText().contains("retryPayment();"));
            button(panel, "收藏").doClick();
        });
        EdtTestUtil.runInEdtAndWait(() -> PlatformTestUtil.waitWithEventsDispatching("Favorite not updated", () -> button(panel, "取消收藏") != null, 15));
        JsonObject get = CoreClient.request("record_get"); get.add("id", snippet.get("id"));
        assertTrue(call(get).getAsJsonObject().getAsJsonObject("input").get("starred").getAsBoolean());
        EdtTestUtil.runInEdtAndWait(() -> {
            button(panel, "复制").doClick();
            assertEquals("retryPayment();", CopyPasteManager.getInstance().getContents(DataFlavor.stringFlavor));
            TestDialogs.set(TestDialog.YES, getTestRootDisposable());
            button(panel.recordMenu(), "删除").doClick();
        });
        awaitStatus(panel, "没有匹配结果");
        try { call(get); fail("Deleted record remained in storage"); }
        catch (java.util.concurrent.ExecutionException expected) { assertTrue(expected.getCause().getMessage().contains("NOT_FOUND")); }
    }
    public void testPersonalLibraryWithoutDemoControlsAndRenderedPanel() throws Exception {
        seed("snippet", "我的资料", "realCode();");
        CodeMoriPanel panel = panel(); awaitStatus(panel, "共 1 条");
        EdtTestUtil.runInEdtAndWait(() -> {
            JPanel root = new JPanel(new java.awt.BorderLayout()); root.add(panel); root.setSize(570, 920);
            layout(root);
            BufferedImage image = new BufferedImage(570, 920, BufferedImage.TYPE_INT_RGB);
            var graphics = image.createGraphics(); root.printAll(graphics); graphics.dispose();
            Path output = Path.of("build/reports/ui/codemori-panel.png"); Files.createDirectories(output.getParent()); ImageIO.write(image, "png", output.toFile());
            assertNull(button(panel.moreMenu(), "加载独立示例"));
            assertNull(button(panel.moreMenu(), "移除全部示例"));
        });
        awaitStatus(panel, "共 1 条");
        JsonObject request = CoreClient.request("search");
        assertEquals(1, call(request).getAsJsonObject().get("total").getAsInt());
    }
    public void testEditorPreservesDraftOnActualConflict() throws Exception {
        JsonObject saved = seed("snippet", "旧标题", "original();");
        JsonObject updated = saved.getAsJsonObject("input").deepCopy(); updated.addProperty("title", "其他客户端的新标题");
        JsonObject update = CoreClient.request("record_update"); update.add("id", saved.get("id")); update.add("revision", saved.get("revision")); update.add("record", updated); call(update);
        AtomicReference<RecordEditorDialog> editor = new AtomicReference<>(); AtomicReference<JComponent> form = new AtomicReference<>(); AtomicReference<String> error = new AtomicReference<>();
        EdtTestUtil.runInEdtAndWait(() -> {
            RecordEditorDialog dialog = new RecordEditorDialog(getProject(), "编辑片段", saved.getAsJsonObject("input"), List.of("重试"), null, (input, owner) -> {
                CompletableFuture.runAsync(() -> {
                    JsonObject request = CoreClient.request("record_update"); request.add("id", saved.get("id")); request.add("revision", saved.get("revision")); request.add("record", input);
                    try { client.call(request, null); }
                    catch (Exception failure) {
                        ApplicationManager.getApplication().invokeLater(() -> { owner.failed(failure.getMessage()); error.set(failure.getMessage()); }, ModalityState.any());
                    }
                });
            });
            editor.set(dialog); form.set(dialog.createCenterPanel());
            named(form.get(), "codemori.title", JTextField.class).setText("仍需保留的输入");
            dialog.doOKAction();
        });
        EdtTestUtil.runInEdtAndWait(() -> {
            PlatformTestUtil.waitWithEventsDispatching("Missing conflict", () -> error.get() != null, 15);
            try {
                assertTrue(error.get().contains("CONFLICT")); assertFalse(editor.get().isDisposed()); assertTrue(editor.get().isOKActionEnabled());
                assertEquals("仍需保留的输入", named(form.get(), "codemori.title", JTextField.class).getText());
            } finally { editor.get().disposeIfNeeded(); }
        });
    }
    public void testEditorSavesActualFormFields() throws Exception {
        AtomicReference<JsonObject> saved = new AtomicReference<>();
        AtomicReference<Exception> failure = new AtomicReference<>();
        AtomicReference<RecordEditorDialog> editor = new AtomicReference<>();
        EdtTestUtil.runInEdtAndWait(() -> {
            JsonObject input = new JsonObject(); input.addProperty("kind", "snippet");
            RecordEditorDialog dialog = new RecordEditorDialog(getProject(), "保存代码片段", input, List.of("支付"), null, (edited, owner) -> {
                CompletableFuture.runAsync(() -> {
                    JsonObject request = CoreClient.request("record_create"); request.add("record", edited);
                    try { saved.set(client.call(request, null).getAsJsonObject()); }
                    catch (Exception error) { failure.set(error); }
                });
            });
            editor.set(dialog); JComponent form = dialog.createCenterPanel();
            named(form, "codemori.title", JTextField.class).setText("支付快照");
            named(form, "codemori.content", JTextArea.class).setText("retryPayment(\"中文\");");
            named(form, "codemori.description", JTextArea.class).setText("只在超时后重试");
            dialog.doOKAction();
        });
        EdtTestUtil.runInEdtAndWait(() -> {
            PlatformTestUtil.waitWithEventsDispatching("Form save did not finish", () -> saved.get() != null || failure.get() != null, 15);
            try {
                assertNull(failure.get());
                assertEquals("支付快照", saved.get().getAsJsonObject("input").get("title").getAsString());
                assertEquals("retryPayment(\"中文\");", saved.get().getAsJsonObject("input").get("content").getAsString());
                assertEquals("只在超时后重试", saved.get().getAsJsonObject("input").get("description").getAsString());
            } finally { editor.get().disposeIfNeeded(); }
        });
    }
    public void testEditorSelectionCaptureKeepsSnapshotAndSavedLine() throws Exception {
        myFixture.configureByText(com.intellij.openapi.fileTypes.PlainTextFileType.INSTANCE, "first line\n支付重试();\nlast line");
        EdtTestUtil.runInEdtAndWait(() -> {
            var editor = myFixture.getEditor();
            editor.getSelectionModel().setSelection(11, 18);
            SelectionContext context = SelectionContext.capture(getProject(), editor);
            assertEquals("支付重试();", context.code()); assertEquals(Integer.valueOf(2), context.line());
            editor.getSelectionModel().removeSelection();
            assertEquals("支付重试();", context.code());
        });
    }

    public void testInstalledPluginResolvesItsBundledCore() throws Exception {
        String previous = System.getProperty("codemori.testDataDir");
        System.setProperty("codemori.testDataDir", storage.resolve("bundled-store").toString());
        try {
            var descriptor = com.intellij.ide.plugins.PluginManagerCore.getPlugin(com.intellij.openapi.extensions.PluginId.getId("com.lusyne.codemori"));
            assertNotNull(descriptor);
            Path jar;
            try (var paths = Files.list(descriptor.getPluginPath().resolve("lib"))) {
                var jars = new java.util.ArrayList<Path>();
                for (Path candidate : paths.filter(p -> p.toString().endsWith(".jar")).toList()) {
                    try (var archive = new java.util.jar.JarFile(candidate.toFile())) {
                        if (archive.getEntry("com/lusyne/codemori/CoreClient.class") != null) jars.add(candidate);
                    }
                }
                assertEquals(1, jars.size()); jar = jars.getFirst();
            }
            // Load the installed JAR itself, rather than the loose Gradle test classes.
            try (var loader = new java.net.URLClassLoader(new java.net.URL[]{jar.toUri().toURL()}, getClass().getClassLoader()) {
                @Override protected Class<?> loadClass(String name, boolean resolve) throws ClassNotFoundException {
                    if (!name.startsWith("com.lusyne.codemori.")) return super.loadClass(name, resolve);
                    synchronized (getClassLoadingLock(name)) {
                        Class<?> loaded = findLoadedClass(name);
                        if (loaded == null) loaded = findClass(name);
                        if (resolve) resolveClass(loaded);
                        return loaded;
                    }
                }
                @Override public java.net.URL getResource(String name) {
                    return name.startsWith("com/lusyne/codemori/") ? findResource(name) : super.getResource(name);
                }
            }) {
                var core = loader.loadClass(CoreClient.class.getName());
                Object installed = core.getMethod("bundled").invoke(null);
                JsonElement response = CompletableFuture.supplyAsync(() -> {
                    try { return (JsonElement) core.getMethod("call", JsonObject.class, com.intellij.openapi.progress.ProgressIndicator.class).invoke(installed, CoreClient.request("search"), null); }
                    catch (Exception error) { throw new java.util.concurrent.CompletionException(error); }
                }).get(20, TimeUnit.SECONDS);
                assertEquals(0, response.getAsJsonObject().get("total").getAsInt());
            }
            assertTrue(Files.exists(storage.resolve("bundled-store/codemori.sqlite3")));
        } finally {
            if (previous == null) System.clearProperty("codemori.testDataDir");
            else System.setProperty("codemori.testDataDir", previous);
        }
    }
    public void testDocumentFormSavesOnlySummaryAndLink() throws Exception {
        AtomicReference<JsonObject> saved = new AtomicReference<>(); AtomicReference<Exception> failure = new AtomicReference<>();
        AtomicReference<RecordEditorDialog> editor = new AtomicReference<>();
        EdtTestUtil.runInEdtAndWait(() -> {
            JsonObject input = new JsonObject(); input.addProperty("kind", "document");
            RecordEditorDialog dialog = new RecordEditorDialog(getProject(), "关联文档", input, List.of(), "PaymentService.java", (edited, owner) -> {
                CompletableFuture.runAsync(() -> {
                    JsonObject request = CoreClient.request("record_create"); request.add("record", edited);
                    try { saved.set(client.call(request, null).getAsJsonObject()); }
                    catch (Exception error) { failure.set(error); }
                });
            });
            editor.set(dialog); JComponent form = dialog.createCenterPanel();
            named(form, "codemori.title", JTextField.class).setText("支付设计");
            named(form, "codemori.url", JTextField.class).setText("https://example.com/notes");
            named(form, "codemori.description", JTextArea.class).setText("这里仅保存手写摘要。");
            dialog.doOKAction();
        });
        EdtTestUtil.runInEdtAndWait(() -> {
            PlatformTestUtil.waitWithEventsDispatching("Document save did not finish", () -> saved.get() != null || failure.get() != null, 15);
            try {
                assertNull(failure.get());
                JsonObject input = saved.get().getAsJsonObject("input");
                assertEquals("https://example.com/notes", input.get("url").getAsString());
                assertEquals("这里仅保存手写摘要。", input.get("description").getAsString());
                assertEquals("", input.get("content").getAsString());
            } finally { editor.get().disposeIfNeeded(); }
        });
    }

    public void testOneClickConfirmsCurrentFileBatchWithoutSecondDialog() throws Exception {
        Path root=storage.resolve("batch repo");Files.createDirectories(root);root=root.toRealPath();Files.writeString(root.resolve("A.java"),"class A {}\n");
        JsonObject register=CoreClient.request("workspace_register");register.addProperty("root",root.toString());String workspace=client.call(register,null).getAsJsonObject().get("id").getAsString();
        for(int n=0;n<2;n++){
            JsonObject create=CoreClient.request("record_create"),input=new JsonObject();input.addProperty("kind","document");input.addProperty("title","Design "+n);input.addProperty("url","https://example.com/batch/"+n);create.add("record",input);String id=client.call(create,null).getAsJsonObject().get("id").getAsString();
            JsonObject link=CoreClient.request("document_link"),binding=new JsonObject();binding.addProperty("workspace_id",workspace);binding.addProperty("path",n==0?"A.java":".");binding.addProperty("kind",n==0?"file":"module");binding.addProperty("document_id",id);link.add("binding",binding);client.call(link,null);
        }
        Files.writeString(root.resolve("A.java"),"class A { } // formatting\n");SelectionContext context=new SelectionContext(root.toString(),"A.java",1,"");
        CodeMoriPanel panel=EdtTestUtil.runInEdtAndGet(()->{CodeMoriPanel value=new CodeMoriPanel(getProject(),client,ignored->{},()->context,dialog->{});Disposer.register(getTestRootDisposable(),value);return value;});
        awaitStatus(panel,"共 2 条");EdtTestUtil.runInEdtAndWait(()->{
            TestDialogs.set(message->{fail("Batch confirmation must not ask a second question");return 0;},getTestRootDisposable());
            JButton button=named(panel,"codemori.reviewCurrent",JButton.class);assertTrue(button.isEnabled());assertTrue(button.getText().contains("2 项"));button.doClick();
            PlatformTestUtil.waitWithEventsDispatching("Batch acknowledgement missing",()->named(panel,"codemori.batchReviewStatus",JTextArea.class).getText().contains("已确认 2 项"),15);
        });
    }
    public void testModuleSaveShowsOriginAndConfirmsChangedCodeThroughMenu() throws Exception {
        Path root=storage.resolve("module repo");Files.createDirectories(root.resolve("pay"));root=root.toRealPath();Files.writeString(root.resolve("pay/A.java"),"class A {}\n");
        Path projectRoot=root; SelectionContext context=new SelectionContext(root.toString(),"pay/A.java",1,"");
        AtomicReference<RecordEditorDialog> editor=new AtomicReference<>();
        CodeMoriPanel panel=EdtTestUtil.runInEdtAndGet(()->{CodeMoriPanel p=new CodeMoriPanel(getProject(),client,ignored->{},()->context,editor::set);Disposer.register(getTestRootDisposable(),p);return p;});
        awaitStatus(panel,"没有匹配结果");EdtTestUtil.runInEdtAndWait(()->panel.associate(context));
        EdtTestUtil.runInEdtAndWait(()->{
            PlatformTestUtil.waitWithEventsDispatching("No module form",()->editor.get()!=null,15);JComponent form=editor.get().createCenterPanel();
            named(form,"codemori.bindingKind",JComboBox.class).setSelectedIndex(1);named(form,"codemori.title",JTextField.class).setText("Module design");named(form,"codemori.url",JTextField.class).setText("https://example.com/module");editor.get().doOKAction();
        });
        awaitStatus(panel,"共 1 条");Files.writeString(projectRoot.resolve("pay/A.java"),"class A { int changed; }\n");
        EdtTestUtil.runInEdtAndWait(()->button(panel.moreMenu(),"刷新资料").doClick());awaitStatus(panel,"共 1 条");
        EdtTestUtil.runInEdtAndWait(()->{
            named(panel,"codemori.documents",JList.class).setSelectedIndex(0);
            TestDialogs.set(TestDialog.YES,getTestRootDisposable());button(panel.recordMenu(),"确认说明仍适用").doClick();
        });
        EdtTestUtil.runInEdtAndWait(()->PlatformTestUtil.waitWithEventsDispatching("Review not confirmed",()->{
            JList<?> list=named(panel,"codemori.documents",JList.class);if(list.getModel().getSize()==0)return false;
            var label=firstDocumentLabel(list);
            return label.getText().contains("继承自目录：pay")&&label.getText().contains("代码未变更");
        },15));
    }
    public void testProjectSaveScopeBindingAndGitChangeConflictThroughPanel() throws Exception {
        var oldInput = TestDialogManager.setTestInputDialog(message -> "IDEA QA");
        Disposer.register(getTestRootDisposable(), () -> TestDialogManager.setTestInputDialog(oldInput));
        Path root = storage.resolve("shared project"); Files.createDirectories(root.resolve("docs"));
        root = root.toRealPath();
        Files.writeString(root.resolve("Payment.java"), "class Payment {}\n");
        Files.writeString(root.resolve("docs/guide.md"), "# Shared guide\n");
        Path projectRoot = root;
        com.intellij.openapi.vfs.newvfs.impl.VfsRootAccess.allowRootAccess(getTestRootDisposable(), root.toString());
        SelectionContext selection = new SelectionContext(root.toString(), "Payment.java", 1, "retry();");
        AtomicReference<RecordEditorDialog> dialog = new AtomicReference<>();
        CodeMoriPanel panel = EdtTestUtil.runInEdtAndGet(() -> {
            CodeMoriPanel value = new CodeMoriPanel(getProject(), client, ignored -> {}, () -> selection, dialog::set);
            Disposer.register(getTestRootDisposable(), value); return value;
        });
        awaitStatus(panel, "没有匹配结果");
        assertFalse(Files.exists(root.resolve(".codemori")));
        EdtTestUtil.runInEdtAndWait(() -> button(panel, "保存选中代码").doClick());
        EdtTestUtil.runInEdtAndWait(() -> {
            PlatformTestUtil.waitWithEventsDispatching("No save editor", () -> dialog.get() != null, 15);
            JComponent form = dialog.get().createCenterPanel();
            JComboBox<?> scope = named(form, "codemori.scope", JComboBox.class);
            assertEquals(0, scope.getSelectedIndex());
            scope.setSelectedIndex(1); named(form, "codemori.title", JTextField.class).setText("Team snippet");
            dialog.get().doOKAction();
        });
        awaitStatus(panel, "共 1 条");
        assertEquals(0, call(CoreClient.request("search")).getAsJsonObject().get("total").getAsInt());
        EdtTestUtil.runInEdtAndWait(() -> {
            assertTrue(named(panel, "codemori.preview", JTextArea.class).getText().contains("项目共享"));
            assertFalse(button(panel, "收藏").isVisible()); button(panel, "复制").doClick();
        });
        awaitStatus(panel, "已复制代码快照");
        EdtTestUtil.runInEdtAndWait(() -> assertEquals("retry();", CopyPasteManager.getInstance().getContents(DataFlavor.stringFlavor)));
        dialog.set(null);
        EdtTestUtil.runInEdtAndWait(() -> button(panel, "关联文档").doClick());
        EdtTestUtil.runInEdtAndWait(() -> {
            PlatformTestUtil.waitWithEventsDispatching("No document editor", () -> dialog.get() != null, 15);
            JComponent form = dialog.get().createCenterPanel();
            assertEquals(0, named(form, "codemori.scope", JComboBox.class).getSelectedIndex());
            named(form, "codemori.scope", JComboBox.class).setSelectedIndex(1);
            named(form, "codemori.title", JTextField.class).setText("Team guide");
            named(form, "codemori.url", JTextField.class).setText(projectRoot.resolve("docs/guide.md").toString());
            dialog.get().doOKAction();
        });
        awaitStatus(panel, "共 2 条");
        EdtTestUtil.runInEdtAndWait(() -> {
            JList<?> links = named(panel, "codemori.documents", JList.class);
            assertEquals(1, links.getModel().getSize());
            assertTrue(((KnowledgeRecord)links.getModel().getElementAt(0)).shared());
        });
        Path shared = root.resolve(".codemori/shared.json");
        var virtualShared = EdtTestUtil.runInEdtAndGet(() -> com.intellij.openapi.vfs.LocalFileSystem.getInstance().refreshAndFindFileByNioFile(shared));
        assertNotNull(virtualShared);
        String bytes = Files.readString(shared); assertTrue(bytes.contains("./docs/guide.md")); assertTrue(bytes.contains("Payment.java"));
        assertFalse(bytes.contains(root.toString())); assertFalse(bytes.contains("workspace_id"));
        // The editor must keep its observed file version even if Git changes the file.
        dialog.set(null);
        EdtTestUtil.runInEdtAndWait(() -> button(panel, "编辑").doClick());
        AtomicReference<JComponent> editingForm = new AtomicReference<>();
        EdtTestUtil.runInEdtAndWait(() -> {
            assertNotNull(dialog.get()); JComponent form = dialog.get().createCenterPanel(); editingForm.set(form);
            assertEquals(1, named(form, "codemori.scope", JComboBox.class).getSelectedIndex());
            assertFalse(named(form, "codemori.scope", JComboBox.class).isEnabled());
            named(form, "codemori.title", JTextField.class).setText("Unsaved team draft");
        });
        JsonObject branch = com.google.gson.JsonParser.parseString(bytes).getAsJsonObject();
        for (JsonElement entry : branch.getAsJsonArray("records")) entry.getAsJsonObject().getAsJsonObject("input").addProperty("description", "Git checkout changed the file");
        String afterGit = branch.toString(); Files.writeString(shared, afterGit);
        EdtTestUtil.runInEdtAndWait(() -> virtualShared.refresh(false, false));
        EdtTestUtil.runInEdtAndWait(() -> PlatformTestUtil.waitWithEventsDispatching("Shared file change did not refresh the panel", () ->
                named(panel, "codemori.preview", JTextArea.class).getText().contains("Git checkout changed the file"), 15));
        EdtTestUtil.runInEdtAndWait(() -> dialog.get().doOKAction());
        EdtTestUtil.runInEdtAndWait(() -> {
            PlatformTestUtil.waitWithEventsDispatching("Draft did not unlock after conflict", () -> dialog.get().isOKActionEnabled(), 15);
            assertFalse(dialog.get().isDisposed());
            assertEquals("Unsaved team draft", named(editingForm.get(), "codemori.title", JTextField.class).getText());
            assertFalse(named(editingForm.get(), "codemori.scope", JComboBox.class).isEnabled());
            dialog.get().disposeIfNeeded();
        });
        assertEquals(afterGit, Files.readString(shared));
        EdtTestUtil.runInEdtAndWait(() -> button(panel.moreMenu(), "刷新资料").doClick());
        awaitStatus(panel, "共 2 条");
        EdtTestUtil.runInEdtAndWait(() -> {
            TestDialogs.set(TestDialog.YES, getTestRootDisposable()); button(panel.recordMenu(), "删除").doClick();
        });
        awaitStatus(panel, "共 1 条");
        assertEquals(0, call(CoreClient.request("search")).getAsJsonObject().get("total").getAsInt());
    }

    public void testRemovingLastTaggedRecordClearsFilterAndShowsRemainingData() throws Exception {
        JsonObject snippet = seed("snippet", "Tagged snippet", "keep();");
        seed("document", "Remaining document", "Summary");
        JsonObject input = snippet.getAsJsonObject("input").deepCopy();
        com.google.gson.JsonArray tags = new com.google.gson.JsonArray(); tags.add("only-tag"); input.add("tags", tags);
        JsonObject update = CoreClient.request("record_update"); update.add("id", snippet.get("id")); update.add("revision", snippet.get("revision")); update.add("record", input); call(update);
        CodeMoriPanel panel = panel(); awaitStatus(panel, "共 2 条");
        EdtTestUtil.runInEdtAndWait(() -> named(panel, "codemori.tag", JComboBox.class).setSelectedItem("only-tag"));
        awaitStatus(panel, "共 1 条");
        EdtTestUtil.runInEdtAndWait(() -> {
            TestDialogs.set(TestDialog.YES, getTestRootDisposable());
            button(panel.recordMenu(), "删除").doClick();
        });
        EdtTestUtil.runInEdtAndWait(() -> PlatformTestUtil.waitWithEventsDispatching("Tag filter was not cleared", () ->
                named(panel, "codemori.tag", JComboBox.class).getSelectedIndex() == 0
                && named(panel, "codemori.preview", JTextArea.class).getText().contains("Remaining document"), 15));
        awaitStatus(panel, "共 1 条");
    }

    public void testFeishuButtonResolvesAgainAndKeepsBrowserOpening() throws Exception {
        JsonObject input = new JsonObject(); input.addProperty("kind", "document");
        input.addProperty("title", "Feishu note");
        String original = "https://tenant.feishu.cn/docx/abc?from=codemori#part";
        input.addProperty("url", original);
        JsonObject create = CoreClient.request("record_create"); create.add("record", input);
        JsonObject record = call(create).getAsJsonObject();
        AtomicReference<String> opened = new AtomicReference<>();
        CodeMoriPanel panel = EdtTestUtil.runInEdtAndGet(() -> {
            CodeMoriPanel value = new CodeMoriPanel(getProject(), client, opened::set);
            Disposer.register(getTestRootDisposable(), value); return value;
        });
        awaitStatus(panel, "共 1 条");
        EdtTestUtil.runInEdtAndWait(() -> PlatformTestUtil.waitWithEventsDispatching("Feishu action unavailable", () ->
                button(panel, "在飞书中打开").isVisible() && button(panel, "在飞书中打开").isEnabled(), 15));
        EdtTestUtil.runInEdtAndWait(() -> button(panel, "在飞书中打开").doClick());
        awaitStatus(panel, "已请求飞书打开");
        assertEquals("feishu://applink.feishu.cn/client/web_url/open?mode=window&url=https%3A%2F%2Ftenant.feishu.cn%2Fdocx%2Fabc%3Ffrom%3Dcodemori%23part", opened.get());
        opened.set(null);
        EdtTestUtil.runInEdtAndWait(() -> button(panel, "浏览器打开").doClick());
        EdtTestUtil.runInEdtAndWait(() -> PlatformTestUtil.waitWithEventsDispatching("Browser target missing", () -> original.equals(opened.get()), 15));
        JsonObject get = CoreClient.request("record_get"); get.add("id", record.get("id"));
        assertEquals(record, call(get).getAsJsonObject());
        input.addProperty("url", "https://notion.so/new");
        JsonObject update = CoreClient.request("record_update"); update.add("id", record.get("id"));
        update.add("revision", record.get("revision")); update.add("record", input); call(update);
        opened.set(null);
        EdtTestUtil.runInEdtAndWait(() -> button(panel, "在飞书中打开").doClick());
        awaitStatus(panel, "记录已被其他客户端修改");
        assertNull("A stale click must not dispatch the cached AppLink", opened.get());
        EdtTestUtil.runInEdtAndWait(() -> {
            JTextField query = named(panel, "codemori.search", JTextField.class);
            query.setText("Feishu note"); query.postActionEvent();
        });
        awaitStatus(panel, "共 1 条");
        EdtTestUtil.runInEdtAndWait(() -> assertFalse(button(panel, "在飞书中打开").isVisible()));
    }

    public void testMarkdownPreviewIsReadonlyAndDoesNotBecomeSearchable() throws Exception {
        com.intellij.openapi.vfs.newvfs.impl.VfsRootAccess.allowRootAccess(getTestRootDisposable(), storage.toRealPath().toString());
        Path note = storage.resolve("中文 notes.md");
        String body = "# HiddenBody\n\n**bold**\n\n<script>alert(1)</script>\n\n![alt](https://example.com/image.png)";
        Files.writeString(note, body);
        JsonObject input = new JsonObject(); input.addProperty("kind", "document");
        input.addProperty("title", "Local note"); input.addProperty("url", note.toString());
        JsonObject request = CoreClient.request("record_create"); request.add("record", input); call(request);
        CodeMoriPanel panel = panel(); awaitStatus(panel, "共 1 条");
        EdtTestUtil.runInEdtAndWait(() -> button(panel.recordMenu(), "Markdown 只读预览").doClick());
        awaitStatus(panel, "只读预览");
        EdtTestUtil.runInEdtAndWait(() -> {
            JEditorPane rendered = named(panel, "codemori.markdown", JEditorPane.class);
            assertFalse(rendered.isEditable());
            assertTrue(rendered.getText().contains("HiddenBody"));
            assertFalse(rendered.getText().contains("<script>"));
            assertFalse(rendered.getText().contains("<img"));
        });
        assertEquals(body, Files.readString(note));
        EdtTestUtil.runInEdtAndWait(() -> button(panel, "打开来源 / 原文").doClick());
        EdtTestUtil.runInEdtAndWait(() -> PlatformTestUtil.waitWithEventsDispatching("Local original document did not open", () -> {
            var editor = com.intellij.openapi.fileEditor.FileEditorManager.getInstance(getProject()).getSelectedTextEditor();
            if (editor == null || !body.equals(editor.getDocument().getText())) return false;
            var file = com.intellij.openapi.fileEditor.FileDocumentManager.getInstance().getFile(editor.getDocument());
            return file != null && file.getName().equals("中文 notes.md");
        }, 15));
        JsonObject search = CoreClient.request("search"); JsonObject filter = new JsonObject();
        filter.addProperty("query", "HiddenBody"); search.add("filter", filter);
        assertEquals(0, call(search).getAsJsonObject().get("total").getAsInt());
    }

    public void testIndexingAndThirdSourceSearchThroughPanel() throws Exception {
        Path project = storage.resolve("project"); Files.createDirectories(project);
        Files.writeString(project.resolve("main.py"), "# 支付注释索引\nsecret = 'NOT_COMMENT'\n");
        CodeMoriPanel panel = panel();
        EdtTestUtil.runInEdtAndWait(() -> panel.indexComments(new SelectionContext(project.toString(), "main.py", 1, ""), true));
        EdtTestUtil.runInEdtAndWait(() -> PlatformTestUtil.waitWithEventsDispatching("Index result not displayed", () ->
                named(panel, "codemori.indexReport", JTextArea.class).getText().contains("更新 1"), 15));
        EdtTestUtil.runInEdtAndWait(() -> {
            JTextField query = named(panel, "codemori.search", JTextField.class); query.setText("支付注释"); query.postActionEvent();
        });
        EdtTestUtil.runInEdtAndWait(() -> PlatformTestUtil.waitWithEventsDispatching("Comment result did not appear", () ->
                named(panel, "codemori.comments", JList.class).getModel().getSize() == 1, 15));
        EdtTestUtil.runInEdtAndWait(() -> {
            JList<?> comments = named(panel, "codemori.comments", JList.class); comments.setSelectedIndex(0);
            assertTrue(named(panel, "codemori.preview", JTextArea.class).getText().contains("# 支付注释索引"));
            assertFalse(named(panel, "codemori.preview", JTextArea.class).getText().contains("NOT_COMMENT"));
        });
    }

    private static void layout(Container container) {
        container.doLayout(); for (Component child : container.getComponents()) if (child instanceof Container nested) layout(nested);
    }

    public void testHeadlessPrintCannotBecomeVisiblePerformanceEvidence() throws Exception {
        String oldLog = System.getProperty("codemori.performanceLog");
        String oldStore = System.getProperty("codemori.testDataDir");
        Path log = storage.resolve("paint.jsonl");
        System.setProperty("codemori.performanceLog", log.toString());
        System.setProperty("codemori.testDataDir", storage.resolve("store").toString());
        try {
            CodeMoriPanel panel = panel(); awaitStatus(panel, "没有匹配结果");
            EdtTestUtil.runInEdtAndWait(() -> {
                JPanel parent = new JPanel(new java.awt.BorderLayout()); parent.add(panel); parent.setSize(570, 920); layout(parent);
                assertFalse(panel.isShowing());
                BufferedImage image = new BufferedImage(570, 920, BufferedImage.TYPE_INT_RGB);
                var graphics = image.createGraphics();
                try { parent.printAll(graphics); } finally { graphics.dispose(); }
            });
            // Allow any accidentally scheduled writer to execute before checking the guard.
            Thread.sleep(100);
            assertFalse("A headless image render must not be recorded as visible-window latency", Files.exists(log));
        } finally {
            if (oldLog == null) System.clearProperty("codemori.performanceLog"); else System.setProperty("codemori.performanceLog", oldLog);
            if (oldStore == null) System.clearProperty("codemori.testDataDir"); else System.setProperty("codemori.testDataDir", oldStore);
        }
    }

    public void testSearchButtonCancelsThePendingTypingDebounce() throws Exception {
        seed("snippet", "Manual search", "retry();");
        CodeMoriPanel panel = panel(); awaitStatus(panel, "共 1 条");
        java.util.concurrent.atomic.AtomicInteger starts = new java.util.concurrent.atomic.AtomicInteger();
        EdtTestUtil.runInEdtAndWait(() -> {
            JTextArea status = named(panel, "codemori.status", JTextArea.class);
            status.getDocument().addDocumentListener(new javax.swing.event.DocumentListener() {
                public void insertUpdate(javax.swing.event.DocumentEvent event) {
                    if (status.getText().equals("正在查找…")) starts.incrementAndGet();
                }
                public void removeUpdate(javax.swing.event.DocumentEvent event) {}
                public void changedUpdate(javax.swing.event.DocumentEvent event) {}
            });
            named(panel, "codemori.search", JTextField.class).setText("Manual");
            button(panel, "搜索").doClick(0);
            long deadline = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(300);
            PlatformTestUtil.waitWithEventsDispatching("Pending debounce did not settle", () -> System.nanoTime() >= deadline, 3);
        });
        awaitStatus(panel, "共 1 条");
        assertEquals("Explicit search must not be followed by a duplicate delayed request", 1, starts.get());
    }
}
