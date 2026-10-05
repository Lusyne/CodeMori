package com.lusyne.codemori;

import com.google.gson.JsonArray;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.ui.DialogWrapper;
import com.intellij.ui.TextFieldWithAutoCompletion;
import com.intellij.ui.components.JBScrollPane;
import java.awt.BorderLayout;
import java.awt.Dimension;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.awt.Insets;
import java.util.List;
import java.util.function.BiConsumer;
import javax.swing.*;
import org.jetbrains.annotations.Nullable;

public final class RecordEditorDialog extends DialogWrapper {
    private final JComboBox<String> bindingKind = new JComboBox<>(new String[]{"当前文件", "当前目录（含子目录）"});
    private final JsonObject original;
    private final boolean document;
    private final BiConsumer<JsonObject, RecordEditorDialog> save;
    private final JComboBox<String> scope = new JComboBox<>(new String[]{"个人资料（仅本机）", "项目共享（随 Git）"});
    private final boolean scopeChangeAllowed;
    private final String scopeHint;
    private final JTextField title = new JTextField();
    private final JTextField language = new JTextField();
    private final JTextField url = new JTextField();
    private final JTextField sourcePath = new JTextField();
    private final JTextArea content = new JTextArea(12, 60);
    private final JTextArea description = new JTextArea(4, 60);
    private final DefaultListModel<String> tags = new DefaultListModel<>();
    private final TextFieldWithAutoCompletion<String> tagInput;
    private final JList<String> tagList = new JList<>(tags);
    private final String bindingHint;
    private boolean saving;
    private JPanel form;

    RecordEditorDialog(Project project, String caption, JsonObject input, List<String> suggestions,
                       String bindingHint, BiConsumer<JsonObject, RecordEditorDialog> save) {
        this(project, caption, input, suggestions, bindingHint, false, false, false, null, save);
    }
    RecordEditorDialog(Project project, String caption, JsonObject input, List<String> suggestions,
                       String bindingHint, boolean shared, boolean projectAvailable, boolean allowScopeChange,
                       String projectError, BiConsumer<JsonObject, RecordEditorDialog> save) {
        super(project, false);
        this.scopeChangeAllowed = allowScopeChange && projectAvailable;
        if (!projectAvailable && !shared) scope.removeItemAt(1);
        scope.setSelectedIndex(shared ? 1 : 0); scope.setEnabled(scopeChangeAllowed); scope.setName("codemori.scope");
        scopeHint = projectError != null ? "项目共享暂不可用：" + projectError : allowScopeChange
                ? "个人资料仅本机；项目共享写入仓库文件，由你提交 Git。收藏不会共享。" : "编辑会保留资料原来的保存范围。";
        this.original = input.deepCopy();
        this.document = input.get("kind").getAsString().equals("document");
        this.save = save;
        this.bindingHint = bindingHint; bindingKind.setName("codemori.bindingKind");
        title.setName("codemori.title"); title.getAccessibleContext().setAccessibleName("标题");
        content.setName("codemori.content"); content.getAccessibleContext().setAccessibleName("代码快照");
        description.setName("codemori.description"); description.getAccessibleContext().setAccessibleName("场景描述或文档摘要");
        url.setName("codemori.url"); url.getAccessibleContext().setAccessibleName("文档链接");
        tagInput = TextFieldWithAutoCompletion.create(project, suggestions, true, "");
        title.setText(text(input, "title"));
        content.setText(text(input, "content"));
        language.setText(text(input, "language"));
        description.setText(text(input, "description"));
        url.setText(text(input, "url"));
        if (input.has("source") && input.get("source").isJsonObject()) sourcePath.setText(text(input.getAsJsonObject("source"), "path"));
        if (input.has("tags")) input.getAsJsonArray("tags").forEach(t -> tags.addElement(t.getAsString()));
        tagList.setCellRenderer(new LiteralTextRenderer());
        content.setFont(new java.awt.Font(java.awt.Font.MONOSPACED, java.awt.Font.PLAIN, 13));
        description.setLineWrap(true); description.setWrapStyleWord(true);
        setTitle(caption); setOKButtonText("保存"); setResizable(true);
        init();
    }

    @Override protected @Nullable JComponent createCenterPanel() {
        JPanel panel = new JPanel(new GridBagLayout()); form = panel;
        int row = 0;
        row = addRow(panel, row, "保存范围", scope, false);
        JTextArea hint = new JTextArea(scopeHint, 2, 30); hint.setLineWrap(true); hint.setWrapStyleWord(true); hint.setEditable(false); hint.setOpaque(false);
        row = addRow(panel, row, "", hint, false);
        if (bindingHint != null) { row = addRow(panel, row, "关联文件", LiteralTextRenderer.label(bindingHint), false); row = addRow(panel,row,"关联范围",bindingKind,false); }
        row = addRow(panel, row, "标题", title, false);
        if (document) {
            row = addRow(panel, row, "文档链接 / 本地路径", url, false);
            row = addRow(panel, row, "摘要（仅搜索本地填写内容）", new JBScrollPane(description), true);
        } else {
            row = addRow(panel, row, "语言（留空自动识别）", language, false);
            row = addRow(panel, row, "代码快照", new JBScrollPane(content), true);
            row = addRow(panel, row, "场景描述", new JBScrollPane(description), false);
            if (!sourcePath.getText().isEmpty()) row = addRow(panel, row, "来源相对路径（可修复）", sourcePath, false);
        }
        JPanel tagPanel = new JPanel(new BorderLayout(6, 6));
        JPanel add = new JPanel(new BorderLayout(6, 0));
        JButton addTag = new JButton("添加标签"); addTag.addActionListener(e -> addPendingTag());
        add.add(tagInput, BorderLayout.CENTER); add.add(addTag, BorderLayout.EAST);
        tagPanel.add(add, BorderLayout.NORTH);
        tagList.setVisibleRowCount(2); tagPanel.add(new JBScrollPane(tagList), BorderLayout.CENTER);
        JButton remove = new JButton("移除选中标签"); remove.addActionListener(e -> {
            for (String tag : tagList.getSelectedValuesList()) tags.removeElement(tag);
        });
        tagPanel.add(remove, BorderLayout.SOUTH);
        addRow(panel, row, "标签（输入时补全已有标签）", tagPanel, false);
        panel.setPreferredSize(new Dimension(650, document ? 485 : 685));
        return panel;
    }

    private static int addRow(JPanel panel, int row, String label, JComponent field, boolean grow) {
        GridBagConstraints c = new GridBagConstraints();
        c.gridx = 0; c.gridy = row; c.weightx = 0; c.anchor = GridBagConstraints.NORTHWEST; c.insets = new Insets(5, 4, 5, 10);
        panel.add(new JLabel(label), c);
        c.gridx = 1; c.weightx = 1; c.weighty = grow ? 1 : 0; c.fill = grow ? GridBagConstraints.BOTH : GridBagConstraints.HORIZONTAL;
        panel.add(field, c);
        return row + 1;
    }
    private static String text(JsonObject value, String name) {
        return value.has(name) && !value.get(name).isJsonNull() ? value.get(name).getAsString() : "";
    }
    private void addPendingTag() {
        String tag = tagInput.getText().trim();
        if (!tag.isEmpty() && !tags.contains(tag)) tags.addElement(tag);
        tagInput.setText("");
    }
    @Override protected void doOKAction() {
        if (saving) return;
        addPendingTag();
        JsonObject input = original.deepCopy();
        input.addProperty("title", title.getText());
        input.addProperty("content", document ? "" : content.getText());
        input.addProperty("language", document ? "" : language.getText());
        input.addProperty("description", description.getText());
        if (document) { input.addProperty("url", url.getText()); input.add("source", JsonNull.INSTANCE); input.addProperty("starred", false); }
        else if (input.has("source") && input.get("source").isJsonObject()) input.getAsJsonObject("source").addProperty("path", sourcePath.getText());
        JsonArray values = new JsonArray(); for (int i=0; i<tags.size(); i++) values.add(tags.get(i)); input.add("tags", values);
        saving = true; setEditingEnabled(form, false); setOKActionEnabled(false); getCancelAction().setEnabled(false); setErrorText(null);
        save.accept(input, this);
    }
    public boolean moduleBinding() { return bindingKind.getSelectedIndex() == 1; }
    public boolean projectScope() { return scope.getSelectedIndex() == 1; }
    public void saved() { if (!isDisposed()) close(OK_EXIT_CODE); }
    public void failed(String message) {
        if (!isDisposed()) { saving = false; setEditingEnabled(form, true); scope.setEnabled(scopeChangeAllowed); setOKActionEnabled(true); getCancelAction().setEnabled(true); setErrorText(message); }
    }
    private static void setEditingEnabled(java.awt.Component component, boolean enabled) {
        if (component == null) return;
        component.setEnabled(enabled);
        if (component instanceof java.awt.Container container) {
            for (java.awt.Component child : container.getComponents()) setEditingEnabled(child, enabled);
        }
    }
    @Override public void doCancelAction() { if (!saving) super.doCancelAction(); }
    @Override public boolean shouldCloseOnCross() { return !saving; }
    @Override public JComponent getPreferredFocusedComponent() { return title; }
}
