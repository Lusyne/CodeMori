package com.lusyne.codemori;

import com.google.gson.*;
import com.intellij.ide.BrowserUtil;
import com.intellij.ide.util.PropertiesComponent;
import com.intellij.openapi.Disposable;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.application.ModalityState;
import com.intellij.openapi.fileChooser.*;
import com.intellij.openapi.fileEditor.*;
import com.intellij.openapi.progress.*;
import com.intellij.openapi.ide.CopyPasteManager;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.ui.Messages;
import com.intellij.openapi.vfs.LocalFileSystem;
import com.intellij.ui.components.JBScrollPane;
import java.awt.*;
import java.awt.datatransfer.StringSelection;
import java.awt.event.*;
import java.io.IOException;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import java.util.function.Supplier;
import com.intellij.openapi.vfs.VirtualFileManager;
import com.intellij.openapi.vfs.newvfs.BulkFileListener;
import com.intellij.openapi.vfs.newvfs.events.VFileEvent;
import javax.swing.*;
import javax.swing.event.DocumentEvent;
import javax.swing.event.DocumentListener;
import org.jetbrains.annotations.NotNull;

public final class CodeMoriPanel extends JPanel implements Disposable {
    private final Project project;
    private final CoreClient client;
    private final Consumer<String> browse;
    private final Supplier<SelectionContext> selectionContext;
    private final Consumer<RecordEditorDialog> showEditor;
    private final JTextField query = new JTextField();
    private final JComboBox<String> libraryScope = new JComboBox<>(new String[]{"个人 + 当前项目共享", "个人资料", "当前项目共享"});
    private final JTextArea projectStatus = new JTextArea(3, 20);
    private final JComboBox<String> kind = new JComboBox<>(new String[]{"全部来源", "代码片段", "文档摘要", "源码注释"});
    private final JComboBox<String> tag = new JComboBox<>(new String[]{"全部标签"});
    private final JCheckBox projectOnly = new JCheckBox("当前项目");
    private final JCheckBox favorites = new JCheckBox("仅个人收藏");
    private final DefaultListModel<Hit> results = new DefaultListModel<>();
    private final JList<Hit> list = new JList<>(results);
    private final ResultRenderer resultRenderer = new ResultRenderer();
    private final DefaultListModel<IndexedComment> commentResults = new DefaultListModel<>();
    private final JList<IndexedComment> commentList = new JList<>(commentResults);
    private final JTabbedPane resultTabs = new JTabbedPane();
    private final JTextArea indexState = new JTextArea(3, 20);
    private final JTextArea indexDetails = new JTextArea(4, 20);
    private final DefaultListModel<KnowledgeRecord> documents = new DefaultListModel<>();
    private final JList<KnowledgeRecord> docList = new JList<>(documents);
    private final JLabel fileLabel = new JLabel("当前文件：未打开");
    private final JTextArea preview = new JTextArea();
    private final JEditorPane markdown = new JEditorPane("text/html", "");
    private final JPanel previews = new JPanel(new CardLayout());
    private final JTextArea status = new JTextArea(2, 20);
    private final JButton copy = new JButton("复制");
    private final JButton open = new JButton("打开来源 / 原文");
    private final JButton feishu = new JButton("在飞书中打开");
    private long targetGeneration;
    private final JButton edit = new JButton("编辑");
    private final JButton star = new JButton("收藏");
    private final JButton previous = new JButton("上一页");
    private final JButton next = new JButton("下一页");
    private final Timer searchTimer;
    private final Timer sharedRefreshTimer;
    private KnowledgeRecord selected;
    private JsonObject selectedBinding;
    private final JButton reviewCurrent = new JButton("一键确认待复核");
    private final JTextArea batchReviewStatus = new JTextArea();
    private boolean reviewingCurrent;
    private final java.util.List<JsonObject> documentEntries = new java.util.ArrayList<>();
    private List<String> tagSuggestions = List.of();
    private long generation;
    private int offset;
    private boolean updating;
    private boolean disposed;
    private String wantedId;
    private FileContext visibleFile;
    private final Path performanceLog;
    private PaintSample pendingPaint;

    private record PaintSample(long generation, long started, String trigger, String query, int total, int rows) {}

    private record Hit(KnowledgeRecord record, JsonArray spans, List<String> workspaceNames) {
        @Override public String toString() { return record.toString(); }
    }
    private record EditorData(JsonObject input, FileContext file, JsonObject projectInfo) {}
    private record FileContext(SelectionContext selection, String workspaceId) {}
    private record ViewData(JsonObject page, JsonObject comments, JsonObject indexStatus, List<KnowledgeRecord> documents, List<JsonObject> entries, FileContext file, List<String> tags, JsonObject projectInfo) {}
    @FunctionalInterface private interface Job<T> { T run(ProgressIndicator indicator) throws Exception; }

    public CodeMoriPanel(Project project) {
        this(project, CoreClient.bundled());
    }
    CodeMoriPanel(Project project, CoreClient client) {
        this(project, client, BrowserUtil::browse);
    }
    CodeMoriPanel(Project project, CoreClient client, Consumer<String> browse) {
        this(project, client, browse, () -> SelectionContext.capture(project));
    }
    CodeMoriPanel(Project project, CoreClient client, Consumer<String> browse, Supplier<SelectionContext> selectionContext) {
        this(project, client, browse, selectionContext, RecordEditorDialog::show);
    }
    CodeMoriPanel(Project project, CoreClient client, Consumer<String> browse, Supplier<SelectionContext> selectionContext, Consumer<RecordEditorDialog> showEditor) {
        super(new BorderLayout(6, 6));
        this.project = project; this.client = client; this.browse = browse; this.selectionContext = selectionContext; this.showEditor = showEditor;
        String measurementPath = System.getProperty("codemori.performanceLog");
        performanceLog = System.getProperty("codemori.testDataDir") != null && measurementPath != null
                && !measurementPath.isBlank() ? Path.of(measurementPath) : null;
        query.setName("codemori.search"); query.getAccessibleContext().setAccessibleName("CodeMori 搜索");
        tag.setName("codemori.tag"); list.setName("codemori.results"); preview.setName("codemori.preview"); status.setName("codemori.status");
        tag.setRenderer(new LiteralTextRenderer());
        setBorder(BorderFactory.createEmptyBorder(8, 8, 8, 8));
        JPanel top = new JPanel(); top.setLayout(new BoxLayout(top, BoxLayout.Y_AXIS));
        JPanel searchBar = new JPanel(new BorderLayout(5, 0));
        query.setToolTipText("搜索代码、标题、标签和手填摘要；多个词需同时匹配");
        searchBar.add(query, BorderLayout.CENTER);
        JButton search = new JButton("搜索"); search.addActionListener(e -> searchNow()); searchBar.add(search, BorderLayout.EAST);
        top.add(searchBar);
        JPanel libraries = new JPanel(new FlowLayout(FlowLayout.LEFT, 4, 2)); libraries.add(new JLabel("资料范围")); libraries.add(libraryScope); top.add(libraries);
        libraryScope.setName("codemori.libraryScope");
        JPanel filters = new JPanel(new FlowLayout(FlowLayout.LEFT, 4, 2));
        filters.add(kind); filters.add(tag); top.add(filters);
        JPanel scopes = new JPanel(new FlowLayout(FlowLayout.LEFT, 4, 2));
        scopes.add(projectOnly); scopes.add(favorites); top.add(scopes);
        JPanel capture = new JPanel(new FlowLayout(FlowLayout.LEFT, 4, 2));
        button(capture, "保存选中代码", () -> capture(selectionContext.get()));
        button(capture, "关联文档", () -> associate(selectionContext.get()));
        JButton more = button(capture, "更多", () -> {});
        more.addActionListener(e -> moreMenu().show(more, 0, more.getHeight())); top.add(capture);
        if (!PropertiesComponent.getInstance().getBoolean("codemori.onboarding.dismissed", false)) {
            JPanel help = new JPanel(new BorderLayout());
            JTextArea text = new JTextArea("选中代码后保存为片段；给当前文件关联笔记。搜索覆盖已保存的片段、手填摘要与手动索引的源码注释。", 3, 20);
            text.setLineWrap(true); text.setWrapStyleWord(true); text.setEditable(false); text.setOpaque(false);
            JButton hide = new JButton("知道了"); hide.addActionListener(e -> {
                PropertiesComponent.getInstance().setValue("codemori.onboarding.dismissed", true);
                help.setVisible(false);
            });
            help.add(text, BorderLayout.CENTER); help.add(hide, BorderLayout.EAST); top.add(help);
        }
        projectStatus.setEditable(false); projectStatus.setLineWrap(true); projectStatus.setWrapStyleWord(true); projectStatus.setOpaque(false);
        projectStatus.setName("codemori.projectStatus"); projectStatus.setVisible(false); top.add(projectStatus);
        add(top, BorderLayout.NORTH);

        list.setSelectionMode(ListSelectionModel.SINGLE_SELECTION);
        list.setCellRenderer(resultRenderer);
        docList.setName("codemori.documents"); docList.setSelectionMode(ListSelectionModel.SINGLE_SELECTION); docList.setVisibleRowCount(3);
        docList.setCellRenderer(new DefaultListCellRenderer() {
            @Override public Component getListCellRendererComponent(JList<?> values, Object value, int index, boolean selected, boolean focus) {
                JLabel label = (JLabel)super.getListCellRendererComponent(values, value, index, selected, focus);
                if (value instanceof KnowledgeRecord record) {
                    String html = record.linkedDocumentHtml(values.getWidth() - 32);
                    if (index >= 0 && index < documentEntries.size()) html = html.replace("</div></html>", "<br>" + KnowledgeRecord.escape(associationLabel(documentEntries.get(index))) + "</div></html>");
                    label.setText(html);
                }
                return label;
            }
        });
        JPanel current = new JPanel(new BorderLayout(0, 4));
        JPanel currentHeader = new JPanel(); currentHeader.setLayout(new BoxLayout(currentHeader, BoxLayout.Y_AXIS)); currentHeader.add(fileLabel);
        reviewCurrent.setName("codemori.reviewCurrent"); reviewCurrent.setEnabled(false); reviewCurrent.addActionListener(e -> reviewCurrent()); currentHeader.add(reviewCurrent);
        reviewCurrent.setToolTipText("点击即确认当前文件的待复核说明仍适用，包含继承的模块关联，模块项按整个目录确认；不会确认整个项目。");
        batchReviewStatus.setName("codemori.batchReviewStatus"); batchReviewStatus.setEditable(false); batchReviewStatus.setLineWrap(true); batchReviewStatus.setWrapStyleWord(true); batchReviewStatus.setOpaque(false); batchReviewStatus.setVisible(false); currentHeader.add(batchReviewStatus);
        current.add(currentHeader, BorderLayout.NORTH); current.add(new JBScrollPane(docList), BorderLayout.CENTER);
        JPanel found = new JPanel(new BorderLayout(0, 5));
        commentList.setName("codemori.comments"); commentList.setSelectionMode(ListSelectionModel.SINGLE_SELECTION);
        commentList.setCellRenderer(new DefaultListCellRenderer() {
            @Override public Component getListCellRendererComponent(JList<?> values, Object value, int index, boolean selected, boolean focus) {
                JLabel label = (JLabel)super.getListCellRendererComponent(values, value, index, selected, focus);
                if (value instanceof IndexedComment comment) label.setText(comment.html());
                return label;
            }
        });
        indexState.setEditable(false); indexState.setOpaque(false); indexState.setLineWrap(true); indexState.setWrapStyleWord(true);
        JPanel indexed = new JPanel(new BorderLayout()); indexed.add(indexState, BorderLayout.NORTH); indexed.add(new JBScrollPane(commentList), BorderLayout.CENTER);
        indexDetails.setName("codemori.indexReport"); indexDetails.setEditable(false); indexDetails.setLineWrap(true); indexDetails.setWrapStyleWord(true);
        indexed.add(new JBScrollPane(indexDetails), BorderLayout.SOUTH);
        resultTabs.addTab("片段 / 摘要", new JBScrollPane(list)); resultTabs.addTab("源码注释", indexed);
        found.add(resultTabs, BorderLayout.CENTER);
        JPanel page = new JPanel(new FlowLayout(FlowLayout.RIGHT)); page.add(previous); page.add(next); found.add(page, BorderLayout.SOUTH);
        JSplitPane currentAndResults = new JSplitPane(JSplitPane.VERTICAL_SPLIT, current, found);
        currentAndResults.setResizeWeight(0.2); currentAndResults.setDividerLocation(120);
        preview.setEditable(false); preview.setFont(new Font(Font.MONOSPACED, Font.PLAIN, 13)); preview.setText("选择结果查看代码或文档摘要。");
        markdown.setEditable(false); markdown.setName("codemori.markdown");
        previews.add(new JBScrollPane(preview), "summary");
        previews.add(new JBScrollPane(markdown), "markdown");
        JSplitPane split = new JSplitPane(JSplitPane.VERTICAL_SPLIT, currentAndResults, previews);
        split.setResizeWeight(0.6); split.setDividerLocation(380); add(split, BorderLayout.CENTER);
        JPanel bottom = new JPanel(new BorderLayout());
        JPanel actions = new JPanel(new GridLayout(2, 3, 4, 4));
        actions.add(copy); actions.add(open); actions.add(feishu); actions.add(edit); actions.add(star);
        feishu.setToolTipText("尝试唤起已安装的飞书客户端；未打开时可使用浏览器入口");
        JButton itemMore = button(actions, "操作…", () -> {}); itemMore.addActionListener(e -> recordMenu().show(itemMore, 0, itemMore.getHeight()));
        bottom.add(actions, BorderLayout.NORTH);
        status.setEditable(false); status.setOpaque(false); status.setLineWrap(true); status.setWrapStyleWord(true);
        bottom.add(status, BorderLayout.SOUTH); add(bottom, BorderLayout.SOUTH);
        selected(null, null);

        searchTimer = new Timer(100, e -> refresh(true, false, "debounce")); searchTimer.setRepeats(false);
        sharedRefreshTimer = new Timer(100, e -> refresh(false, true)); sharedRefreshTimer.setRepeats(false);
        query.getDocument().addDocumentListener(new DocumentListener() {
            public void insertUpdate(DocumentEvent e) { changed(); }
            public void removeUpdate(DocumentEvent e) { changed(); }
            public void changedUpdate(DocumentEvent e) { changed(); }
            private void changed() { if (!updating) searchTimer.restart(); }
        });
        query.addActionListener(e -> searchNow());
        libraryScope.addActionListener(e -> { if (!updating) { if (libraryScope.getSelectedIndex() == 2) favorites.setSelected(false); refresh(true, true); } });
        kind.addActionListener(e -> { if (!updating) refresh(true, false); });
        tag.addActionListener(e -> { if (!updating) refresh(true, false); });
        projectOnly.addActionListener(e -> refresh(true, false)); favorites.addActionListener(e -> refresh(true, false));
        previous.addActionListener(e -> { offset = Math.max(0, offset - 50); refresh(false, false); });
        next.addActionListener(e -> { offset += 50; refresh(false, false); });
        list.addListSelectionListener(e -> {
            if (!e.getValueIsAdjusting() && !updating && list.getSelectedValue() != null) {
                docList.clearSelection(); selected(list.getSelectedValue().record(), null);
            }
        });
        docList.addListSelectionListener(e -> {
            if (!e.getValueIsAdjusting() && !updating && docList.getSelectedValue() != null && visibleFile != null) {
                list.clearSelection(); selected(docList.getSelectedValue(), documentEntries.get(docList.getSelectedIndex()).getAsJsonObject("binding").deepCopy());
            }
        });
        commentList.addListSelectionListener(e -> {
            if (!e.getValueIsAdjusting() && !updating && commentList.getSelectedValue() != null) {
                IndexedComment comment = commentList.getSelectedValue();
                list.clearSelection(); docList.clearSelection(); selected(null, null);
                preview.setText(comment.location() + "\n索引于 " + comment.indexedAt() + "\n\n" + comment.text());
                preview.setCaretPosition(0);
            }
        });
        commentList.addMouseListener(new MouseAdapter() { @Override public void mouseClicked(MouseEvent e) { if (e.getClickCount() == 2) openComment(); } });
        commentList.getInputMap().put(KeyStroke.getKeyStroke(KeyEvent.VK_ENTER, 0), "openComment");
        commentList.getActionMap().put("openComment", new AbstractAction() { public void actionPerformed(ActionEvent e) { openComment(); } });
        copy.addActionListener(e -> copySelected()); open.addActionListener(e -> openSelected());
        edit.addActionListener(e -> editSelected()); star.addActionListener(e -> toggleStar());
        feishu.addActionListener(e -> openInFeishu());
        list.addMouseListener(new MouseAdapter() {
            @Override public void mouseClicked(MouseEvent e) { if (e.getClickCount() == 2) openSelected(); }
            @Override public void mousePressed(MouseEvent e) { popup(e); }
            @Override public void mouseReleased(MouseEvent e) { popup(e); }
            private void popup(MouseEvent e) {
                if (e.isPopupTrigger()) {
                    int index = list.locationToIndex(e.getPoint());
                    if (index >= 0) list.setSelectedIndex(index);
                    recordMenu().show(list, e.getX(), e.getY());
                }
            }
        });
        list.getInputMap().put(KeyStroke.getKeyStroke(KeyEvent.VK_ENTER, 0), "open");
        list.getActionMap().put("open", new AbstractAction() { public void actionPerformed(ActionEvent e) { openSelected(); } });
        project.getMessageBus().connect(this).subscribe(FileEditorManagerListener.FILE_EDITOR_MANAGER, new FileEditorManagerListener() {
            @Override public void selectionChanged(@NotNull FileEditorManagerEvent event) { refresh(false, false); }
        });
        ApplicationManager.getApplication().getMessageBus().connect(this).subscribe(VirtualFileManager.VFS_CHANGES, new BulkFileListener() {
            @Override public void after(@NotNull List<? extends VFileEvent> events) {
                List<String> paths = events.stream().map(VFileEvent::getPath).toList();
                ApplicationManager.getApplication().invokeLater(() -> {
                    if (disposed) return;
                    String root = visibleFile == null ? selectionContext.get().root() : visibleFile.selection().root();
                    if (root == null) return;
                    Path shared = Path.of(root).resolve(".codemori/shared.json").normalize();
                    if (paths.stream().anyMatch(value -> { Path changed = Path.of(value).normalize(); return shared.equals(changed) || shared.startsWith(changed) || changed.startsWith(Path.of(root)); })) sharedRefreshTimer.restart();
                }, ModalityState.nonModal());
            }
        });
        refresh(true, true);
    }

    public void focusSearch() { query.requestFocusInWindow(); query.selectAll(); }
    private static JButton button(JPanel panel, String label, Runnable action) {
        JButton button = new JButton(label); button.addActionListener(e -> action.run()); panel.add(button); return button;
    }
    private <T> void background(String label, boolean cancellable, Job<T> work, Consumer<T> success, Consumer<Exception> failure) {
        ModalityState modality = ModalityState.current();
        new Task.Backgroundable(project, label, cancellable) {
            @Override public void run(@NotNull ProgressIndicator indicator) {
                try {
                    T value = work.run(indicator);
                    ApplicationManager.getApplication().invokeLater(() -> { if (!disposed && !project.isDisposed()) success.accept(value); }, modality);
                } catch (Exception error) {
                    ApplicationManager.getApplication().invokeLater(() -> { if (!disposed && !project.isDisposed()) failure.accept(error); }, modality);
                }
            }
        }.queue();
    }
    private void failed(Exception error) { status.setText(errorText(error)); }
    private static String errorText(Exception error) {
        if (error instanceof ProcessCanceledException) return "操作已取消。若刚才正在保存，请刷新确认结果后再重试。";
        String message = error.getMessage() == null ? error.getClass().getSimpleName() : error.getMessage();
        if (message.contains("Project shared file changed")) return "项目共享文件已变更，当前草稿仍保留。请复制需要的内容后刷新，再重新编辑。";
        return message.contains("CONFLICT") ? "记录已被其他客户端修改。请刷新后重新编辑；当前输入仍保留。\n" + message : message;
    }
    private FileContext context(SelectionContext selection, ProgressIndicator indicator) throws Exception {
        if (selection.root() == null || !Files.isDirectory(Path.of(selection.root()))) return new FileContext(selection, null);
        JsonObject request = CoreClient.request("workspace_register"); request.addProperty("root", selection.root());
        JsonObject workspace = client.call(request, indicator).getAsJsonObject();
        return new FileContext(new SelectionContext(workspace.get("root").getAsString(), selection.path(), selection.line(), selection.code()), workspace.get("id").getAsString());
    }
    private static JsonObject binding(FileContext context, String documentId) {
        JsonObject binding = new JsonObject(); binding.addProperty("workspace_id", context.workspaceId());
        binding.addProperty("path", context.selection().path()); binding.addProperty("document_id", documentId); return binding;
    }
    private static List<KnowledgeRecord> records(JsonArray data) {
        List<KnowledgeRecord> result = new ArrayList<>(); data.forEach(item -> result.add(KnowledgeRecord.from(item))); return result;
    }

    private String libraryKey() { return switch (libraryScope.getSelectedIndex()) { case 1 -> "personal"; case 2 -> "project"; default -> "all"; }; }
    private static JsonObject objectOrNull(JsonObject value, String field) { return value.has(field) && value.get(field).isJsonObject() ? value.getAsJsonObject(field) : null; }
    private JsonObject scopeRequest(JsonObject request, KnowledgeRecord record) {
        if (record == null || !record.shared()) return request;
        JsonObject scoped = CoreClient.request("project"); scoped.addProperty("root", record.projectRoot()); scoped.addProperty("expected_version", record.projectVersion()); scoped.add("request", request); return scoped;
    }
    private JsonObject projectInfo(FileContext file, ProgressIndicator indicator) throws Exception {
        if (file == null || file.workspaceId() == null) return null;
        JsonObject request = CoreClient.request("project_info"); request.addProperty("root", file.selection().root()); return client.call(request, indicator).getAsJsonObject();
    }
    private void refresh(boolean resetPage, boolean reloadTags) {
        refresh(resetPage, reloadTags, "refresh");
    }
    private void searchNow() {
        searchTimer.stop();
        refresh(true, false, "submit");
    }
    private void refresh(boolean resetPage, boolean reloadTags, String trigger) {
        if (disposed) return;
        long started = performanceLog == null ? 0 : System.nanoTime();
        pendingPaint = null;
        if (resetPage) offset = 0;
        long ticket = ++generation;
        SelectionContext selection = selectionContext.get();
        if (visibleFile != null && (!java.util.Objects.equals(visibleFile.selection().root(), selection.root())
                || !java.util.Objects.equals(visibleFile.selection().path(), selection.path()))) {
            documents.clear(); documentEntries.clear(); updateReviewCurrent(); if (selectedBinding != null) selected(null, null);
            selectedBinding = null; fileLabel.setText("正在读取当前文件…");
        }
        JsonObject filter = new JsonObject(); filter.addProperty("query", query.getText()); filter.addProperty("limit", 50); filter.addProperty("offset", offset);
        if (kind.getSelectedIndex() > 0 && kind.getSelectedIndex() < 3) filter.addProperty("kind", kind.getSelectedIndex() == 1 ? "snippet" : "document");
        if (tag.getSelectedIndex() > 0) filter.addProperty("tag", (String)tag.getSelectedItem());
        filter.addProperty("starred", favorites.isSelected()); filter.addProperty("demo", false);
        boolean scoped = projectOnly.isSelected();
        String library = libraryKey();
        boolean onlyComments = kind.getSelectedIndex() == 3;
        String selectId = wantedId != null ? wantedId : selected == null ? null : selected.key(); wantedId = null;
        status.setText("正在查找…");
        background("CodeMori 搜索", true, indicator -> {
            FileContext file = context(selection, indicator);
            if (scoped) {
                if (file.workspaceId() == null) throw new IllegalArgumentException("当前项目没有本地目录，请使用全部资料。");
                filter.addProperty("workspace_id", file.workspaceId());
            }
            List<String> tags = null;
            if (reloadTags) {
                JsonObject get = CoreClient.request("library_tags"); get.addProperty("demo", false); get.addProperty("root", file.selection().root()); get.addProperty("scope", library);
                tags = KnowledgeRecord.strings(client.call(get, indicator).getAsJsonObject().getAsJsonArray("tags"));
                if (filter.has("tag")) {
                    String selectedTag = filter.get("tag").getAsString().toLowerCase(java.util.Locale.ROOT);
                    if (tags.stream().noneMatch(value -> value.toLowerCase(java.util.Locale.ROOT).equals(selectedTag))) filter.remove("tag");
                }
            }
            JsonObject request = CoreClient.request("library_search"); request.add("filter", filter); request.addProperty("root", file.selection().root()); request.addProperty("scope", library);
            JsonObject result = onlyComments ? emptyPage() : client.call(request, indicator).getAsJsonObject();
            JsonObject sharedInfo = objectOrNull(result, "project");
            JsonObject commentPage = emptyPage();
            if (!filter.has("kind") && !filter.has("tag") && !filter.get("starred").getAsBoolean()) {
                JsonObject commentFilter = new JsonObject();
                for (String key : List.of("query", "workspace_id", "limit", "offset")) if (filter.has(key)) commentFilter.add(key, filter.get(key));
                JsonObject searchComments = CoreClient.request("comments_search"); searchComments.add("filter", commentFilter);
                commentPage = client.call(searchComments, indicator).getAsJsonObject();
            }
            JsonObject indexedStatus = null;
            if (file.workspaceId() != null) {
                JsonObject readStatus = CoreClient.request("comments_status"); readStatus.addProperty("workspace_id", file.workspaceId());
                indexedStatus = client.call(readStatus, indicator).getAsJsonObject();
            }
            List<KnowledgeRecord> linked = new ArrayList<>(); List<JsonObject> entries = new ArrayList<>();
            if (selection.hasFile() && file.workspaceId() != null) {
                JsonObject read = CoreClient.request("library_file_documents"); read.addProperty("root", file.selection().root()); read.addProperty("workspace_id", file.workspaceId()); read.addProperty("path", selection.path());
                JsonObject links = client.call(read, indicator).getAsJsonObject();
                for (JsonElement entry : links.getAsJsonArray("entries")) { entries.add(entry.getAsJsonObject()); linked.add(KnowledgeRecord.from(entry.getAsJsonObject().get("record"))); }
                sharedInfo = objectOrNull(links, "project");
            }
            return new ViewData(result, commentPage, indexedStatus, linked, entries, file, tags, sharedInfo);
        }, data -> {
            if (ticket != generation) return;
            updating = true;
            try {
                JsonObject sharedInfo = data.projectInfo();
                boolean projectError = sharedInfo != null && sharedInfo.has("error") && !sharedInfo.get("error").isJsonNull();
                boolean projectExists = sharedInfo != null && sharedInfo.get("exists").getAsBoolean();
                projectStatus.setVisible(projectError || projectExists || library.equals("project"));
                projectStatus.setText(projectError ? "项目共享暂不可用：" + sharedInfo.get("error").getAsString() + "。请修复 .codemori/shared.json 后刷新；个人资料仍可使用。"
                        : projectExists ? "项目共享已加载；修改写入 .codemori/shared.json，提交 Git 后同事可获取。" : "当前项目暂无共享资料；保存时选择“项目共享”即可创建。");
                visibleFile = data.file(); documents.clear(); documentEntries.clear(); documentEntries.addAll(data.entries()); data.documents().forEach(documents::addElement); updateReviewCurrent();
                fileLabel.setText(selection.hasFile() ? data.file().workspaceId() == null ? "工作区目录不可用，请从“更多”重新定位。" : "当前文件：" + selection.path() + "（" + documents.size() + " 篇关联文档）" : "打开项目中的文件以查看关联文档");
                if (data.tags() != null) {
                    String oldTag = (String)tag.getSelectedItem(); tagSuggestions = data.tags();
                    tag.removeAllItems(); tag.addItem("全部标签"); tagSuggestions.forEach(tag::addItem);
                    if (oldTag != null) tagSuggestions.stream().filter(value -> value.toLowerCase(java.util.Locale.ROOT).equals(oldTag.toLowerCase(java.util.Locale.ROOT))).findFirst().ifPresent(tag::setSelectedItem);
                }
                resultRenderer.clear();
                results.clear();
                for (JsonElement entry : data.page().getAsJsonArray("items")) {
                    JsonObject hit = entry.getAsJsonObject(); results.addElement(new Hit(KnowledgeRecord.from(hit.get("record")), hit.getAsJsonArray("excerpt"), hit.has("workspace_names") ? KnowledgeRecord.strings(hit.getAsJsonArray("workspace_names")) : List.of()));
                }
                commentResults.clear(); data.comments().getAsJsonArray("items").forEach(value -> commentResults.addElement(new IndexedComment(value.getAsJsonObject())));
                int commentTotal = data.comments().get("total").getAsInt();
                resultTabs.setTitleAt(1, "源码注释（" + commentTotal + "）");
                if (onlyComments) resultTabs.setSelectedIndex(1);
                indexState.setText((data.indexStatus() == null ? "当前项目尚未索引。" : "当前项目：" + data.indexStatus().get("files").getAsInt() + " 个文件 / " + data.indexStatus().get("comments").getAsInt() + " 条注释。") +
                    "注释仅在本机索引；从“更多”增量索引。Java、JS/TS、Python；忽略隐藏、Git 忽略和构建目录。");
                selected(null, null);
                int selectedIndex = -1;
                for (int i = 0; i < results.size(); i++) if (results.get(i).record().key().equals(selectId)) selectedIndex = i;
                if (selectedIndex < 0 && !results.isEmpty()) selectedIndex = 0;
                if (selectedIndex >= 0) { list.setSelectedIndex(selectedIndex); selected(results.get(selectedIndex).record(), null); }
                int total = data.page().get("total").getAsInt(); previous.setEnabled(offset > 0); next.setEnabled(offset + 50 < Math.max(total, commentTotal));
                status.setText(total == 0 && commentTotal == 0 ? emptyResultText(filter, onlyComments, library) : "共 " + total + " 条资料 / " + commentTotal + " 条注释 · 第 " + (offset/50 + 1) + " 页 · 文档仅检索手填摘要");
                if (performanceLog != null) {
                    pendingPaint = new PaintSample(ticket, started, trigger, filter.get("query").getAsString(), total, results.size());
                    repaint();
                }
            } finally { updating = false; }
        }, error -> { if (ticket == generation) failed(error); });
    }

    @Override public void paint(Graphics graphics) {
        super.paint(graphics);
        PaintSample sample = pendingPaint;
        if (sample == null || sample.generation() != generation || disposed || !isShowing() || isPaintingForPrint()) return;
        pendingPaint = null;
        double elapsed = (System.nanoTime() - sample.started()) / 1_000_000.0;
        Window window = SwingUtilities.getWindowAncestor(this);
        JsonObject measurement = new JsonObject();
        measurement.addProperty("generation", sample.generation());
        measurement.addProperty("trigger", sample.trigger());
        measurement.addProperty("query", sample.query()); measurement.addProperty("total", sample.total());
        measurement.addProperty("rows", sample.rows()); measurement.addProperty("elapsed_ms", elapsed);
        measurement.addProperty("showing", isShowing()); measurement.addProperty("window_active", window != null && window.isActive());
        measurement.addProperty("width", getWidth()); measurement.addProperty("height", getHeight());
        // Only explicit QA profiles emit timing data. File I/O is outside the measured EDT paint.
        ApplicationManager.getApplication().executeOnPooledThread(() -> {
            try {
                Files.writeString(performanceLog, measurement + "\n", StandardCharsets.UTF_8,
                        StandardOpenOption.CREATE, StandardOpenOption.APPEND);
            } catch (IOException error) {
                com.intellij.openapi.diagnostic.Logger.getInstance(CodeMoriPanel.class).warn("Could not write QA paint timing", error);
            }
        });
    }

    static String emptyResultText(JsonObject filter, boolean onlyComments) { return emptyResultText(filter, onlyComments, "personal"); }
    static String emptyResultText(JsonObject filter, boolean onlyComments, String library) {
        List<String> scope = new ArrayList<>();
        scope.add(switch (library) { case "project" -> "项目共享"; case "all" -> "个人 + 当前项目共享"; default -> "个人资料"; });
        scope.add(filter.has("workspace_id") ? "当前项目" : "全部项目");
        scope.add(onlyComments ? "源码注释" : !filter.has("kind") ? "全部来源" : filter.get("kind").getAsString().equals("snippet") ? "代码片段" : "文档摘要");
        if (filter.has("tag")) scope.add("标签：" + filter.get("tag").getAsString());
        if (filter.has("starred") && filter.get("starred").getAsBoolean()) scope.add("仅个人收藏");
        return "没有匹配结果。当前范围：" + String.join(" · ", scope) + "。可清除筛选、保存片段或关联文档。";
    }

    private void selected(KnowledgeRecord record, JsonObject fromBinding) {
        selected = record; selectedBinding = fromBinding;
        long targetTicket = ++targetGeneration;
        feishu.setVisible(false); feishu.setEnabled(false); open.setText("打开来源 / 原文");
        ((CardLayout)previews.getLayout()).show(previews, "summary");
        copy.setEnabled(record != null); open.setEnabled(record != null); edit.setEnabled(record != null); star.setEnabled(record != null && !record.document() && !record.shared());
        star.setText(record != null && record.starred() ? "取消收藏" : "收藏");
        star.setVisible(record == null || !record.shared());
        if (record == null) { preview.setText("选择结果查看代码或文档摘要。"); return; }
        preview.setLineWrap(record.document()); preview.setWrapStyleWord(record.document());
        String heading = record.scopeLabel() + " · " + record.field("title") + "\n" + (record.shared() ? record.attribution() + "\n" : "") + "标签：" + String.join(" · ", record.tags()) + "\n\n";
        if (record.document()) preview.setText(heading + "手填摘要（不包含外部正文）\n" + record.field("description") + "\n\n原文链接：\n" + record.field("url"));
        else {
            String source = "";
            JsonObject input = record.input();
            if (input.has("source") && input.get("source").isJsonObject()) {
                JsonObject origin = input.getAsJsonObject("source"); source = "\n\n保存时来源：" + origin.get("path").getAsString();
                if (origin.has("line") && !origin.get("line").isJsonNull()) source += ":" + origin.get("line").getAsInt();
            }
            preview.setText(heading + record.field("description") + "\n\n" + record.field("language") + "\n" + record.field("content") + source);
        }
        if (fromBinding != null) {
            for (JsonObject entry : documentEntries) if (entry.getAsJsonObject("binding").equals(fromBinding)) { preview.append("\n\n" + associationLabel(entry)); break; }
        }
        preview.setCaretPosition(0);
        if (record.document()) background("读取文档打开方式", true,
                indicator -> documentTargets(record, indicator), targets -> {
            if (targetTicket != targetGeneration) return;
            boolean available = targets.has("feishu_applink") && !targets.get("feishu_applink").isJsonNull();
            feishu.setVisible(available); feishu.setEnabled(available);
            if (available) open.setText("浏览器打开");
            revalidate(); repaint();
        }, error -> { if (targetTicket == targetGeneration) failed(error); });
    }

    public void capture(SelectionContext selection) {
        if (selection.code().isBlank()) { status.setText("请先在编辑器中选中代码，或从“更多”新建片段。"); return; }
        createSnippet(selection);
    }
    private void createSnippet(SelectionContext selection) {
        background("准备代码片段", true, indicator -> {
            FileContext file = context(selection, indicator);
            JsonObject input = new JsonObject(); input.addProperty("kind", "snippet"); input.addProperty("content", selection.code());
            if (selection.hasFile() && file.workspaceId() != null) {
                JsonObject source = new JsonObject(); source.addProperty("workspace_id", file.workspaceId()); source.addProperty("path", selection.path()); source.addProperty("line", selection.line()); input.add("source", source);
            }
            return new EditorData(input, file, projectInfo(file, indicator));
        }, data -> editor("保存代码片段", data.input(), null, null, data.projectInfo()), this::failed);
    }
    public void associate(SelectionContext selection) {
        if (!selection.hasFile()) { status.setText("请先打开项目中的源码文件。"); return; }
        background("准备关联文档", true, indicator -> {
            FileContext file = context(selection, indicator);
            if (file.workspaceId() == null) throw new IOException("工作区目录不可用，请先重新定位工作区。");
            JsonObject input = new JsonObject(); input.addProperty("kind", "document");
            return new EditorData(input, file, projectInfo(file, indicator));
        }, data -> editor("关联外部文档", data.input(), null, data.file(), data.projectInfo()), this::failed);
    }
    private void editor(String title, JsonObject input, KnowledgeRecord existing, FileContext associate, JsonObject sharing) {
        RecordEditorDialog dialog = new RecordEditorDialog(project, title, input, tagSuggestions,
                associate == null ? null : associate.selection().path(), existing != null && existing.shared(),
                sharing != null && (!sharing.has("error") || sharing.get("error").isJsonNull()), existing == null,
                sharing != null && sharing.has("error") && !sharing.get("error").isJsonNull() ? sharing.get("error").getAsString() : null, (updated, owner) -> {
            boolean shared = owner.projectScope(); boolean module = owner.moduleBinding();
            Runnable save = () -> background("保存 CodeMori 资料", false, indicator -> {
                if (existing != null && shared != existing.shared()) throw new IllegalArgumentException("编辑已有资料不能改变保存范围。");
                JsonObject request = CoreClient.request(shared ? "project_save" : existing == null ? "record_create" : "record_update");
                if (shared) {
                    if (sharing == null || (sharing.has("error") && !sharing.get("error").isJsonNull())) throw new IllegalArgumentException("当前项目共享不可用。");
                    request.addProperty("root", sharing.get("root").getAsString()); request.addProperty("expected_version", sharing.get("version").getAsString());
                    if (associate != null) { request.addProperty("binding_path", module ? parentPath(associate.selection().path()) : associate.selection().path()); request.addProperty("binding_kind", module ? "module" : "file"); }
                }
                if (existing != null) { request.addProperty("id", existing.id()); request.addProperty("revision", existing.revision()); }
                request.add("record", updated);
                KnowledgeRecord saved = KnowledgeRecord.from(client.call(request, indicator));
                if (associate != null && !shared) {
                    JsonObject link = CoreClient.request("document_link"); JsonObject association = binding(associate, saved.id()); if (module) { association.addProperty("path", parentPath(associate.selection().path())); association.addProperty("kind", "module"); } link.add("binding", association); client.call(link, indicator);
                }
                return saved;
            }, saved -> {
                owner.saved(); DocumentHints.get(project).invalidate(); updating = true; libraryScope.setSelectedIndex(saved.shared() ? 2 : 1);
                if (existing == null) { query.setText(""); kind.setSelectedIndex(0); tag.setSelectedIndex(0); favorites.setSelected(false); projectOnly.setSelected(false); }
                updating = false; wantedId = saved.key(); refresh(true, true);
            }, error -> owner.failed(errorText(error)));
            if (shared) withIdentity(save, error -> owner.failed(errorText(error))); else save.run();
        });
        showEditor.accept(dialog);
    }
    private void editSelected() {
        if (selected == null) return;
        JsonObject sharing = null;
        if (selected.shared()) { sharing = new JsonObject(); sharing.addProperty("root", selected.projectRoot()); sharing.addProperty("version", selected.projectVersion()); }
        editor(selected.document() ? "编辑文档摘要" : "编辑代码片段", selected.input(), selected, null, sharing);
    }
    private void toggleStar() {
        if (selected == null || selected.document() || selected.shared()) return;
        KnowledgeRecord item = selected;
        JsonObject input = item.input(); input.addProperty("starred", !item.starred());
        JsonObject request = CoreClient.request("record_update"); request.addProperty("id", item.id()); request.addProperty("revision", item.revision()); request.add("record", input);
        mutation("更新收藏", request, () -> { wantedId = item.key(); refresh(false, false); });
    }
    private void deleteSelected() {
        if (selected == null) return;
        KnowledgeRecord item = selected;
        if (Messages.showYesNoDialog(project, "删除“" + item.field("title") + "”？此操作不会删除来源代码或外部文档。", "删除资料", Messages.getQuestionIcon()) != Messages.YES) return;
        JsonObject request = CoreClient.request("record_delete"); request.addProperty("id", item.id()); request.addProperty("revision", item.revision());
        mutation("删除资料", scopeRequest(request, item), () -> refresh(true, true));
    }
    private void copySelected() {
        if (selected == null) return;
        KnowledgeRecord record = selected;
        if (record.shared()) {
            JsonObject get = CoreClient.request("record_get"); get.addProperty("id", record.id());
            background("复制项目资料", true, indicator -> {
                KnowledgeRecord latest = KnowledgeRecord.from(client.call(scopeRequest(get, record), indicator));
                if (latest.revision() != record.revision()) throw new IllegalArgumentException("CONFLICT: Record changed");
                return latest;
            }, this::copyRecord, this::failed);
        } else copyRecord(record);
    }
    private void copyRecord(KnowledgeRecord record) {
        CopyPasteManager.getInstance().setContents(new StringSelection(record.field(record.document() ? "url" : "content")));
        status.setText(record.document() ? "已复制文档链接。" : "已复制代码快照。");
    }
    private JsonObject documentTargets(KnowledgeRecord record, ProgressIndicator indicator) throws Exception {
        JsonObject request = CoreClient.request("document_open_targets");
        request.addProperty("id", record.id()); request.addProperty("revision", record.revision());
        return client.call(scopeRequest(request, record), indicator).getAsJsonObject();
    }
    private void openInFeishu() {
        if (selected == null || !selected.document()) return;
        KnowledgeRecord record = selected;
        background("在飞书中打开", true, indicator -> documentTargets(record, indicator), targets -> {
            try {
                if (!targets.has("feishu_applink") || targets.get("feishu_applink").isJsonNull())
                    throw new IllegalArgumentException("此文档不支持飞书客户端打开，请使用原文入口。");
                URI uri = URI.create(targets.get("feishu_applink").getAsString());
                if (!"feishu".equals(uri.getScheme()) || !"applink.feishu.cn".equals(uri.getAuthority())
                        || !"/client/web_url/open".equals(uri.getPath()))
                    throw new IllegalArgumentException("核心返回了不支持的飞书入口，请更新插件。");
                browse.accept(uri.toString());
                status.setText("已请求飞书打开；若未唤起，可点击“浏览器打开”。");
            } catch (Exception error) { failed(error); }
        }, this::failed);
    }
    private void openSelected() {
        if (selected == null) return;
        KnowledgeRecord record = selected;
        if (record.document()) {
            JsonObject request = CoreClient.request("document_target");
            request.addProperty("id", record.id()); request.addProperty("revision", record.revision());
            background("打开原文", true, indicator -> client.call(scopeRequest(request, record), indicator).getAsString(), target -> {
                try {
                    URI uri = URI.create(target);
                    if ("file".equals(uri.getScheme())) {
                        URI local = uri.getAuthority() == null ? uri : new URI("file", null, uri.getPath(), null);
                        openFile(Path.of(local).toString(), null);
                    } else if (List.of("https", "http", "obsidian").contains(uri.getScheme())) browse.accept(uri.toString());
                    else throw new IllegalArgumentException("不支持的文档链接类型。");
                } catch (Exception error) { failed(error); }
            }, this::failed);
        } else {
            JsonObject input = record.input();
            if (!input.has("source") || !input.get("source").isJsonObject()) { status.setText("这个片段没有来源文件。"); return; }
            JsonObject source = input.getAsJsonObject("source");
            JsonObject request = CoreClient.request("source_target"); request.add("source", source);
            background("打开来源文件", true, indicator -> client.call(scopeRequest(request, record), indicator).getAsString(), path -> {
                Integer line = source.has("line") && !source.get("line").isJsonNull() ? source.get("line").getAsInt() : null;
                openFile(path, line);
            }, this::failed);
        }
    }
    private static JsonObject emptyPage() {
        JsonObject result = new JsonObject(); result.add("items", new JsonArray()); result.addProperty("total", 0); return result;
    }
    private void openComment() {
        IndexedComment comment = commentList.getSelectedValue(); if (comment == null) return;
        JsonObject request = CoreClient.request("comments_target"); request.add("source", comment.source());
        background("打开注释来源", true, indicator -> client.call(request, indicator).getAsString(),
            path -> openFile(path, comment.source().get("line").getAsInt()), this::failed);
    }
    private void indexComments(boolean single) {
        indexComments(selectionContext.get(), single);
    }
    void indexComments(SelectionContext selection, boolean single) {
        if (selection.root() == null || (single && !selection.hasFile())) { status.setText("请先打开本地项目中的源码文件。"); return; }
        background("索引已保存的源码注释", true, indicator -> {
            FileContext file = context(selection, indicator);
            if (file.workspaceId() == null) throw new IOException("工作区目录不可用。");
            JsonObject request = CoreClient.request("comments_index"); request.addProperty("workspace_id", file.workspaceId());
            if (single) request.addProperty("path", selection.path());
            return client.call(request, indicator).getAsJsonObject();
        }, report -> {
            String summary = "更新 " + report.get("indexed_files") + "，未变 " + report.get("unchanged_files") + "，清理 " + report.get("removed_files") +
                "，跳过 " + report.get("skipped_files") + "，失败 " + report.get("failed_files") + "。\n忽略规则排除的文件未计入跳过数。\n" +
                String.join("\n", KnowledgeRecord.strings(report.getAsJsonArray("details")));
            refresh(true, false);
            // Preserve diagnostics independently of the asynchronous search's status text.
            indexDetails.setText(summary); indexDetails.setCaretPosition(0); resultTabs.setSelectedIndex(1);
        }, this::failed);
    }
    private void previewMarkdown() {
        if (selected == null || !selected.document()) return;
        KnowledgeRecord record = selected;
        JsonObject request = CoreClient.request("markdown_preview");
        request.addProperty("id", record.id()); request.addProperty("revision", record.revision());
        background("读取本地 Markdown", true, indicator -> client.call(scopeRequest(request, record), indicator).getAsJsonObject(), data -> {
            if (selected == null || !selected.key().equals(record.key()) || selected.revision() != record.revision() || !java.util.Objects.equals(selected.projectVersion(), record.projectVersion())) return;
            markdown.setText("<html><body>" + data.get("html").getAsString() + "</body></html>");
            markdown.setCaretPosition(0);
            ((CardLayout)previews.getLayout()).show(previews, "markdown");
            status.setText("只读预览 · 图片和链接跳转已禁用；正文不会加入搜索。\n" + data.get("path").getAsString());
        }, this::failed);
    }
    private void openFile(String path, Integer line) {
        var file = LocalFileSystem.getInstance().refreshAndFindFileByNioFile(Path.of(path));
        if (file == null || file.isDirectory()) { status.setText("文件不存在。可编辑来源相对路径，或从“更多”重新定位工作区。"); return; }
        new OpenFileDescriptor(project, file, line == null ? 0 : Math.max(0, line - 1), 0).navigate(true);
    }
    private static String parentPath(String path) { int slash = path.lastIndexOf('/'); return slash < 0 ? "." : path.substring(0, slash); }
    private static String associationLabel(JsonObject entry) {
        JsonObject binding = entry.getAsJsonObject("binding"), state = entry.getAsJsonObject("review_state");
        String status = switch (state.get("status").getAsString()) { case "current" -> "代码未变更"; case "needs_review" -> "代码已变化，说明待复核"; case "unavailable" -> "代码暂不可检查"; default -> "尚未确认基线"; };
        String value = (entry.get("inherited").getAsBoolean() ? "继承自目录：" : "文件关联：") + binding.get("path").getAsString() + " · " + status + "（已保存代码）";
        if (binding.has("review") && binding.get("review").isJsonObject()) { JsonObject review = binding.getAsJsonObject("review"); value += " · 上次确认：" + (review.get("confirmed_by").isJsonNull() ? "本机用户" : review.getAsJsonObject("confirmed_by").get("display_name").getAsString()) + " / " + java.time.Instant.ofEpochMilli(review.get("confirmed_at").getAsLong()); }
        return value;
    }
    private List<JsonObject> pendingReviews() {
        return documentEntries.stream().filter(e -> e.getAsJsonObject("review_state").get("status").getAsString().equals("needs_review") && !e.getAsJsonObject("review_state").get("fingerprint").isJsonNull()).toList();
    }
    private void updateReviewCurrent() {
        List<JsonObject> pending = pendingReviews();
        long modules = pending.stream().filter(e -> e.get("inherited").getAsBoolean()).count();
        reviewCurrent.setText("一键确认待复核（" + pending.size() + " 项，含模块 " + modules + " 项）");
        reviewCurrent.setEnabled(!reviewingCurrent && !pending.isEmpty());
    }
    private record BatchReviewResult(int confirmed, List<String> errors) {}
    private void reviewCurrent() {
        if (reviewingCurrent || pendingReviews().isEmpty()) return;
        List<JsonObject> entries = pendingReviews().stream().map(JsonObject::deepCopy).toList();
        String file = visibleFile == null ? "当前文件" : visibleFile.selection().path();
        reviewingCurrent = true; updateReviewCurrent(); batchReviewStatus.setVisible(true); batchReviewStatus.setText("正在确认 " + file + " 的待复核关联（包含继承模块）…");
        Runnable run = () -> background("确认当前文件全部待复核", false, indicator -> {
            var requests = new java.util.LinkedHashMap<String, JsonObject>();
            var counts = new java.util.LinkedHashMap<String, JsonArray>();
            for (JsonObject entry : entries) {
                KnowledgeRecord record = KnowledgeRecord.from(entry.get("record"));
                String key = record.shared() ? record.projectRoot() + ":" + record.projectVersion() : "personal";
                JsonArray items = counts.computeIfAbsent(key, ignored -> new JsonArray());
                JsonObject item = new JsonObject(); item.add("binding", entry.get("binding").deepCopy()); item.add("fingerprint", entry.getAsJsonObject("review_state").get("fingerprint")); item.addProperty("document_revision", record.revision()); items.add(item);
                if (!requests.containsKey(key)) { JsonObject request = CoreClient.request("bindings_review"); request.add("items", items); requests.put(key, scopeRequest(request, record)); }
            }
            int confirmed = 0; List<String> errors = new ArrayList<>();
            for (var group : requests.entrySet()) {
                try { confirmed += client.call(group.getValue(), indicator).getAsJsonObject().get("confirmed").getAsInt(); }
                catch (Exception error) { errors.add((group.getKey().equals("personal") ? "个人资料：" : "项目共享：") + errorText(error)); }
            }
            return new BatchReviewResult(confirmed, errors);
        }, result -> {
            reviewingCurrent = false; DocumentHints.get(project).invalidate(); refresh(false, false);
            batchReviewStatus.setText(file + "：已确认 " + result.confirmed() + " 项。" + (result.errors().isEmpty() ? "" : "\n未完成：" + String.join("\n", result.errors())));
        }, error -> { reviewingCurrent = false; updateReviewCurrent(); batchReviewStatus.setText(errorText(error)); });
        if (entries.stream().anyMatch(e -> KnowledgeRecord.from(e.get("record")).shared())) withIdentity(run, error -> { reviewingCurrent = false; updateReviewCurrent(); batchReviewStatus.setText(errorText(error)); }); else run.run();
    }
    private void reviewSelected(boolean confirm) {
        if (selectedBinding == null || selected == null) return;
        KnowledgeRecord record = selected; JsonObject association = selectedBinding.deepCopy();
        JsonObject get = CoreClient.request("binding_changes"); get.add("binding", association);
        background("读取关联代码变化", true, indicator -> client.call(scopeRequest(get, record), indicator).getAsJsonObject(), data -> {
            if (!confirm) {
                JTextArea text = new JTextArea(data.get("note").getAsString() + "\n\n" + (data.get("diff").isJsonNull() ? "" : data.get("diff").getAsString()), 24, 90); text.setEditable(false);
                com.intellij.openapi.ui.DialogBuilder dialog = new com.intellij.openapi.ui.DialogBuilder(project); dialog.setTitle("关联代码变化"); dialog.setCenterPanel(new JBScrollPane(text)); dialog.removeAllActions(); dialog.addCloseButton(); dialog.show(); return;
            }
            JsonObject state = data.getAsJsonObject("review_state");
            if (state.get("fingerprint").isJsonNull()) { status.setText("代码暂不可检查：" + state.get("error").getAsString()); return; }
            if (Messages.showYesNoDialog(project, "确认关联的已保存代码与这篇说明仍然一致？", "确认说明仍适用", Messages.getQuestionIcon()) != Messages.YES) return;
            JsonObject request = CoreClient.request("binding_review"); request.add("binding", association); request.addProperty("fingerprint", state.get("fingerprint").getAsString()); request.addProperty("document_revision", record.revision());
            mutation("确认说明", scopeRequest(request, record), () -> refresh(false, false));
        }, this::failed);
    }
    void repairPaths() {
        SelectionContext selection = selectionContext.get(); if (selection.root() == null) { status.setText("请先打开本地项目。"); return; }
        int scope = Messages.showDialog(project, "选择修复的资料范围", "修复移动后的关联", new String[]{"个人资料", "项目共享", "取消"}, 0, Messages.getQuestionIcon()); if (scope < 0 || scope == 2) return;
        String from = Messages.showInputDialog(project,"移动前的项目相对文件或目录路径","修复路径",Messages.getQuestionIcon()); if (from == null || from.isBlank()) return;
        String to = Messages.showInputDialog(project,"移动后的项目相对路径（目标必须存在）","修复路径",Messages.getQuestionIcon()); if (to == null || to.isBlank()) return;
        background("预览路径修复", true, indicator -> {
            FileContext file = context(selection, indicator); JsonObject request = CoreClient.request("paths_repair"); request.addProperty("workspace_id",file.workspaceId()); request.addProperty("from",from); request.addProperty("to",to);
            if (scope == 1) { JsonObject info = projectInfo(file,indicator); if (!info.get("error").isJsonNull()) throw new IllegalArgumentException(info.get("error").getAsString()); JsonObject wrapped=CoreClient.request("project"); wrapped.addProperty("root",info.get("root").getAsString()); wrapped.addProperty("expected_version",info.get("version").getAsString()); wrapped.add("request",request); request=wrapped; }
            return new JsonObject[]{request,client.call(request,indicator).getAsJsonObject()};
        }, data -> {
            JsonObject result=data[1]; if(Messages.showYesNoDialog(project,"将修复 "+result.get("bindings").getAsInt()+" 项关联、"+result.get("records").getAsInt()+" 个片段来源。","应用路径修复",Messages.getQuestionIcon())!=Messages.YES)return;
            JsonObject request=data[0]; (scope==1?request.getAsJsonObject("request"):request).addProperty("token",result.get("token").getAsString()); mutation("修复路径",request,()->refresh(true,true));
        },this::failed);
    }
    private void withIdentity(Runnable ready, Consumer<Exception> failure) {
        background("读取共享署名", true, indicator -> client.call(CoreClient.request("identity_get"), indicator).getAsJsonObject().get("author"), value -> {
            if (!value.isJsonNull()) ready.run(); else editIdentity("", ready, failure);
        }, failure);
    }
    void configureIdentity() {
        background("读取共享署名", true, indicator -> client.call(CoreClient.request("identity_get"), indicator).getAsJsonObject().get("author"), value ->
            editIdentity(value.isJsonNull() ? "" : value.getAsJsonObject().get("display_name").getAsString(), () -> status.setText("共享署名已更新；历史记录保留保存时的署名。"), this::failed), this::failed);
    }
    private void editIdentity(String existing, Runnable ready, Consumer<Exception> failure) {
        String name = Messages.showInputDialog(project, "显示名会随共享资料提交到 Git，两端 IDE 共用；署名不是账号认证。", "设置共享资料署名", Messages.getQuestionIcon(), existing, null);
        if (name == null) { failure.accept(new IllegalArgumentException("未设置共享署名，草稿已保留。")); return; }
        JsonObject request = CoreClient.request("identity_set"); request.addProperty("display_name", name);
        background("保存共享署名", false, indicator -> client.call(request, indicator), value -> ready.run(), failure);
    }
    void copyCodeLink(SelectionContext context) {
        if (!context.hasFile()) { status.setText("请先打开项目中的本地源码文件。"); return; }
        int choice = Messages.showDialog(project, "选择文档里的代码打开入口", "复制代码位置链接", new String[]{"当前 JetBrains IDE", "VS Code", "Markdown 双入口", "取消"}, 0, Messages.getQuestionIcon());
        if (choice < 0 || choice == 3) return;
        String product = com.intellij.openapi.application.ApplicationNamesInfo.getInstance().getScriptName();
        JsonObject request = CoreClient.request("code_link_create"); request.addProperty("root", context.root()); request.addProperty("path", context.path());
        request.addProperty("line", context.line() == null ? 1 : context.line()); request.addProperty("jetbrains_product", product);
        background("创建代码位置链接", false, indicator -> client.call(request, indicator).getAsJsonObject(), links -> {
            String idea = links.get("jetbrains_url").getAsString(), code = links.get("vscode_url").getAsString();
            String text = choice == 2 ? "[在 JetBrains 中打开代码](" + idea + ") · [在 VS Code 中打开代码](" + code + ")" : choice == 1 ? code : idea;
            CopyPasteManager.getInstance().setContents(new StringSelection(text));
            status.setText("已复制。请将 .codemori/project.json 与 .codemori/.gitignore 提交到 Git，同事才能定位自己的克隆。");
        }, this::failed);
    }
    void pasteCodeLink() {
        String url = Messages.showInputDialog(project, "粘贴完整的 CodeMori VS Code 或 JetBrains 链接", "打开代码位置链接", Messages.getQuestionIcon());
        if (url == null) return;
        ModalityState modality = ModalityState.current();
        CodeLinks.receive(client, CodeLinks.openProjects(), url.trim()).thenAccept(error -> ApplicationManager.getApplication().invokeLater(() -> {
            if (!disposed) status.setText(error == null ? "已打开代码位置。" : error);
        }, modality));
    }
    private void mutation(String title, JsonObject request, Runnable done) {
        Runnable write = () -> background(title, false, indicator -> client.call(request, indicator), ignored -> { DocumentHints.get(project).invalidate(); done.run(); }, this::failed);
        if (request.get("op").getAsString().equals("project")) withIdentity(write, this::failed); else write.run();
    }
    private void bindSelected() { bindSelected(false); }
    private void bindSelected(boolean module) {
        if (selected == null || !selected.document()) return;
        if (selected.raw().get("is_demo").getAsBoolean()) { status.setText("示例资料不能关联到真实项目。"); return; }
        KnowledgeRecord record = selected; SelectionContext selection = selectionContext.get();
        if (!selection.hasFile()) { status.setText("请先打开项目中的源码文件。"); return; }
        Runnable write = () -> background("关联到当前文件", false, indicator -> {
            FileContext file = context(selection, indicator);
            if (file.workspaceId() == null) throw new IOException("工作区目录不可用，请先重新定位工作区。");
            if (record.shared() && !record.projectRoot().equals(file.selection().root())) throw new IllegalArgumentException("共享文档只能关联同一项目的文件。");
            JsonObject request = CoreClient.request("document_link"); JsonObject association = binding(file, record.id()); if (module) { association.addProperty("path", parentPath(selection.path())); association.addProperty("kind", "module"); } request.add("binding", association); return client.call(scopeRequest(request, record), indicator);
        }, ignored -> refresh(false, false), this::failed);
        if (record.shared()) withIdentity(write, this::failed); else write.run();
    }
    private void unlinkSelected() {
        if (selectedBinding == null || selected == null) return;
        JsonObject request = CoreClient.request("document_unlink"); request.add("binding", selectedBinding.deepCopy());
        mutation("解除关联", scopeRequest(request, selected), () -> refresh(false, false));
    }

    JPopupMenu recordMenu() {
        JPopupMenu menu = new JPopupMenu();
        item(menu, "Markdown 只读预览", this::previewMarkdown).setEnabled(selected != null && selected.localDocument());
        item(menu, "编辑", this::editSelected).setEnabled(selected != null);
        item(menu, "删除", this::deleteSelected).setEnabled(selected != null);
        item(menu, "关联到当前文件", this::bindSelected).setEnabled(selected != null && selected.document());
        item(menu, "关联到当前目录", () -> bindSelected(true)).setEnabled(selected != null && selected.document());
        item(menu, "查看关联代码变化", () -> reviewSelected(false)).setEnabled(selectedBinding != null);
        item(menu, "确认说明仍适用", () -> reviewSelected(true)).setEnabled(selectedBinding != null);
        item(menu, "解除当前文件关联", this::unlinkSelected).setEnabled(selectedBinding != null);
        item(menu, "查看 / 修复关联位置", this::repairBinding).setEnabled(selected != null && selected.document());
        return menu;
    }
    JPopupMenu moreMenu() {
        JPopupMenu menu = new JPopupMenu();
        item(menu, DocumentHints.get(project).enabled() ? "隐藏代码旁文档提示" : "显示代码旁文档提示", () -> DocumentHints.get(project).toggle());
        item(menu, "修复移动后的文件/目录", this::repairPaths);
        item(menu, "复制代码位置链接", () -> copyCodeLink(selectionContext.get()));
        item(menu, "打开代码位置链接", this::pasteCodeLink);
        item(menu, "设置共享署名", this::configureIdentity);
        item(menu, "索引当前文件注释", () -> indexComments(true));
        item(menu, "增量索引当前工作区注释", () -> indexComments(false));
        item(menu, "新建代码片段", () -> createSnippet(new SelectionContext(selectionContext.get().root(), null, null, "")));
        item(menu, "刷新资料", () -> { DocumentHints.get(project).invalidate(); refresh(false, true); });
        menu.addSeparator();
        item(menu, "导出个人资料备份…", this::exportBackup);
        item(menu, "预览并导入个人备份…", this::importBackup);
        item(menu, "重新定位工作区…", this::relocateWorkspace);
        return menu;
    }
    private static JMenuItem item(JPopupMenu menu, String label, Runnable action) {
        JMenuItem item = new JMenuItem(label); item.addActionListener(e -> action.run()); menu.add(item); return item;
    }
    private void exportBackup() {
        var file = FileChooserFactory.getInstance().createSaveFileDialog(PlatformApi.backupDescriptor(), project).save((com.intellij.openapi.vfs.VirtualFile)null, "codemori-backup.json");
        if (file == null) return;
        Path path = file.getFile().toPath();
        if (Files.exists(path) && Messages.showYesNoDialog(project, "覆盖已有备份文件？\n" + path, "确认覆盖", Messages.getQuestionIcon()) != Messages.YES) return;
        background("导出个人资料备份", false, indicator -> {
            JsonElement data = client.call(CoreClient.request("backup_export"), indicator);
            Path parent = path.toAbsolutePath().getParent();
            Path temporary = Files.createTempFile(parent, ".codemori-backup-", ".json");
            try {
                Files.writeString(temporary, new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(data), StandardCharsets.UTF_8);
                try { Files.move(temporary, path, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING); }
                catch (AtomicMoveNotSupportedException ignored) { Files.move(temporary, path, StandardCopyOption.REPLACE_EXISTING); }
            } finally { Files.deleteIfExists(temporary); }
            return path;
        }, saved -> status.setText("已导出个人资料备份：" + saved), this::failed);
    }
    private void importBackup() {
        var descriptor = FileChooserDescriptorFactory.createSingleFileDescriptor("json").withTitle("选择 CodeMori 个人 JSON 备份");
        var file = FileChooser.chooseFile(descriptor, project, null);
        if (file == null) return;
        background("验证备份", true, indicator -> {
            Path path = file.toNioPath();
            if (Files.size(path) > 128L * 1024 * 1024) throw new IOException("备份超过当前 128 MiB 导入限制。");
            JsonElement backup = JsonParser.parseString(Files.readString(path, StandardCharsets.UTF_8));
            JsonObject request = CoreClient.request("backup_preview"); request.add("backup", backup);
            return new JsonElement[]{backup, client.call(request, indicator)};
        }, result -> {
            if (result[1].getAsJsonObject().get("invalid_count").getAsInt() > 0) {
                Messages.showErrorDialog(project, importSummary(result[1].getAsJsonObject()), "备份包含无效数据"); return;
            }
            if (Messages.showYesNoDialog(project, importSummary(result[1].getAsJsonObject()) + "\n\n冲突保留本机版本。确认导入？", "备份预览", Messages.getQuestionIcon()) != Messages.YES) return;
            JsonObject request = CoreClient.request("backup_import"); request.add("backup", result[0]);
            background("导入备份", false, indicator -> client.call(request, indicator).getAsJsonObject(), report -> {
                Messages.showInfoMessage(project, importSummary(report), "导入完成"); refresh(true, true);
            }, this::failed);
        }, this::failed);
    }
    static String importSummary(JsonObject report) {
        int invalid = report.get("invalid_count").getAsInt();
        if (invalid > 0) {
            StringBuilder summary = new StringBuilder("发现 " + invalid + " 项无效数据，未导入。修复后才能计算新增与冲突数量。");
            int shown = 0;
            for (JsonElement value : report.getAsJsonArray("invalid_entries")) {
                if (shown++ == 5) break;
                JsonObject item = value.getAsJsonObject();
                summary.append("\n").append(item.get("kind").getAsString()).append("[").append(item.get("index").getAsInt()).append("] ")
                    .append(item.get("id").getAsString()).append(": ").append(item.get("reason").getAsString());
            }
            return summary.toString();
        }
        StringBuilder summary = new StringBuilder("新增工作区：" + report.get("new_workspaces") + "\n新增资料：" + report.get("new_records")
                + "\n新增关联：" + report.get("new_bindings") + "\n相同记录跳过：" + report.get("unchanged") + "\n冲突：" + report.getAsJsonArray("conflicts").size() + "\n无效：0");
        int shown = 0;
        for (JsonElement conflict : report.getAsJsonArray("conflicts")) {
            if (shown++ == 5) { summary.append("\n…其余冲突均保留本机数据"); break; }
            JsonObject item = conflict.getAsJsonObject(); summary.append("\n").append(item.get("id").getAsString()).append(": ").append(item.get("reason").getAsString());
        }
        return summary.toString();
    }
    private void relocateWorkspace() {
        background("读取工作区", true, indicator -> client.call(CoreClient.request("workspace_list"), indicator).getAsJsonArray(), values -> {
            if (values.isEmpty()) { status.setText("尚无工作区。"); return; }
            String[] choices = new String[values.size()];
            for (int i=0; i<values.size(); i++) { JsonObject w = values.get(i).getAsJsonObject(); choices[i] = "<html>" + KnowledgeRecord.escape(w.get("name").getAsString() + " — " + w.get("root").getAsString()) + "</html>"; }
            Object choice = JOptionPane.showInputDialog(this, "选择要重新定位的工作区", "重新定位工作区", JOptionPane.PLAIN_MESSAGE, null, choices, choices[0]);
            if (choice == null) return;
            int index = java.util.Arrays.asList(choices).indexOf(choice.toString()); JsonObject workspace = values.get(index).getAsJsonObject();
            var directory = FileChooser.chooseFile(FileChooserDescriptorFactory.createSingleFolderDescriptor().withTitle("选择新的工作区根目录"), project, null);
            if (directory == null) return;
            JsonObject request = CoreClient.request("workspace_relocate"); request.add("id", workspace.get("id")); request.add("revision", workspace.get("revision")); request.addProperty("root", directory.getPath());
            mutation("重新定位工作区", request, () -> refresh(false, false));
        }, this::failed);
    }
    private void repairBinding() {
        if (selected == null || !selected.document()) return;
        KnowledgeRecord record = selected;
        JsonObject request = CoreClient.request("document_bindings"); request.addProperty("document_id", record.id());
        background("读取文件关联", true, indicator -> {
            JsonArray workspaces;
            if (record.shared()) {
                workspaces = new JsonArray(); JsonObject workspace = new JsonObject(); workspace.addProperty("id", "project");
                workspace.addProperty("root", record.projectRoot()); workspace.addProperty("name", Path.of(record.projectRoot()).getFileName().toString()); workspaces.add(workspace);
            } else workspaces = client.call(CoreClient.request("workspace_list"), indicator).getAsJsonArray();
            return new JsonArray[]{client.call(scopeRequest(request, record), indicator).getAsJsonArray(), workspaces};
        }, data -> {
            JsonArray values = data[0];
            if (values.isEmpty()) { status.setText("此文档尚未关联文件，可使用“关联到当前文件”。"); return; }
            java.util.Map<String, String> workspaceNames = new java.util.HashMap<>();
            for (JsonElement value : data[1]) {
                JsonObject workspace = value.getAsJsonObject();
                workspaceNames.put(workspace.get("id").getAsString(), workspace.get("name").getAsString() + " — " + workspace.get("root").getAsString());
            }
            String[] choices = new String[values.size()];
            for (int i=0; i<values.size(); i++) {
                JsonObject b = values.get(i).getAsJsonObject(); String workspaceId = b.get("workspace_id").getAsString();
                choices[i] = "<html>" + KnowledgeRecord.escape(b.get("path").getAsString() + " · " + workspaceNames.getOrDefault(workspaceId, workspaceId)) + "</html>";
            }
            Object choice = JOptionPane.showInputDialog(this, "选择要修复的文件关联", "关联位置", JOptionPane.PLAIN_MESSAGE, null, choices, choices[0]);
            if (choice == null) return;
            JsonObject binding = values.get(java.util.Arrays.asList(choices).indexOf(choice.toString())).getAsJsonObject();
            String path = Messages.showInputDialog(project, "输入新的项目相对路径", "修复文件关联", Messages.getQuestionIcon(), binding.get("path").getAsString(), null);
            if (path == null) return;
            JsonObject move = CoreClient.request("binding_move"); move.add("binding", binding); move.addProperty("path", path);
            mutation("修复文件关联", scopeRequest(move, record), () -> refresh(false, false));
        }, this::failed);
    }
    @Override public void dispose() { disposed = true; generation++; searchTimer.stop(); sharedRefreshTimer.stop(); }

    private static final class ResultRenderer extends DefaultListCellRenderer {
        // At most one page of cells. Reuse the HTML view during layout, paint and selection.
        private final java.util.Map<Hit, DefaultListCellRenderer> cells = new java.util.IdentityHashMap<>();
        void clear() { cells.clear(); }
        @Override public Component getListCellRendererComponent(JList<?> list, Object value, int index, boolean selected, boolean focus) {
            if (!(value instanceof Hit hit)) return super.getListCellRendererComponent(list, value, index, selected, focus);
            DefaultListCellRenderer label = cells.computeIfAbsent(hit, ResultRenderer::createCell);
            label.getListCellRendererComponent(list, label.getText(), index, selected, focus);
            label.setBorder(BorderFactory.createEmptyBorder(7, 5, 7, 5));
            return label;
        }
        private static DefaultListCellRenderer createCell(Hit hit) {
            DefaultListCellRenderer label = new DefaultListCellRenderer();
            StringBuilder text = new StringBuilder("<html><div style='width:340px'><b>")
                    .append(KnowledgeRecord.escape(hit.record().scopeLabel() + " · " + hit.record())).append("</b><br>");
            if (hit.record().shared()) text.append(KnowledgeRecord.escape(hit.record().attribution())).append("<br>");
            for (JsonElement span : hit.spans()) {
                JsonObject piece = span.getAsJsonObject(); boolean marked = piece.get("highlight").getAsBoolean();
                if (marked) text.append("<b><u>");
                text.append(KnowledgeRecord.escape(piece.get("text").getAsString()).replace("\n", " "));
                if (marked) text.append("</u></b>");
            }
            if (!hit.workspaceNames().isEmpty()) text.append("<br><small>项目：").append(KnowledgeRecord.escape(String.join(" · ", hit.workspaceNames()))).append("</small>");
            text.append("</div></html>"); label.setText(text.toString());
            return label;
        }
    }
}
