package com.lusyne.codemori;

import com.google.gson.JsonParser;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class KnowledgeRecordTest {
    private static final String RECORD = """
            {"id":"r1","revision":1,"is_demo":false,"input":{"kind":"snippet",
            "title":"支付 <重试>","content":"a < b;","description":"说明","language":"java",
            "tags":["中文"],"starred":true,"source":null,"url":null}}
            """;
    @Test void retainsTheObservedRevisionAndSourceInput() {
        var record = KnowledgeRecord.from(JsonParser.parseString(RECORD));
        var edit = record.input(); edit.addProperty("title", "changed");
        assertEquals(1, record.revision());
        assertEquals("支付 <重试>", record.field("title"));
        assertEquals("中文", record.tags().getFirst());
    }
    @Test void escapesHtmlInSearchResults() {
        assertEquals("&lt;script&gt;&amp;&quot;&#39;", KnowledgeRecord.escape("<script>&\"'"));
    }
    @Test void refusesIncompleteRecords() {
        assertThrows(IllegalArgumentException.class, () -> KnowledgeRecord.from(JsonParser.parseString("{}")));
        assertThrows(IllegalArgumentException.class, () -> KnowledgeRecord.from(JsonParser.parseString(RECORD.replace("\"language\":\"java\"", "\"language\":false"))));
    }
    @Test void currentFileDocumentsShowSourceAndSummaryWithoutSelection() {
        var raw = JsonParser.parseString(RECORD).getAsJsonObject();
        var input = raw.getAsJsonObject("input"); input.addProperty("kind", "document");
        input.addProperty("url", "https://example.com/design?q=<tag>");
        input.addProperty("description", "超时可重试\n业务拒绝 <不重试>");
        String rendered = KnowledgeRecord.from(raw).linkedDocumentHtml(300);
        assertTrue(rendered.contains("支付 &lt;重试&gt;"));
        assertTrue(rendered.contains("来源：https://example.com/design?q=&lt;tag&gt;"));
        assertTrue(rendered.contains("超时可重试<br>业务拒绝 &lt;不重试&gt;"));
        assertFalse(rendered.contains("<tag>"));
        input.addProperty("description", "");
        assertTrue(KnowledgeRecord.from(raw).linkedDocumentHtml(300).contains("未填写摘要"));
    }
    @Test void sharedAuthorsAreVisibleEscapedAndLegacyCreatorIsNotInvented() {
        var raw = JsonParser.parseString(RECORD).getAsJsonObject();
        raw.addProperty("scope", "project"); raw.addProperty("project_root", "/repo"); raw.addProperty("project_version", "hash");
        raw.getAsJsonObject("input").addProperty("kind", "document"); raw.getAsJsonObject("input").addProperty("url", "https://example.com");
        assertTrue(KnowledgeRecord.from(raw).attribution().contains("创建者：历史未署名"));
        raw.add("updated_by", JsonParser.parseString("{\"id\":\"editor\",\"display_name\":\"<img src=x>\"}"));
        String html = KnowledgeRecord.from(raw).linkedDocumentHtml(300);
        assertTrue(html.contains("最近修改：&lt;img src=x&gt;")); assertFalse(html.contains("<img src=x>"));
    }
    @Test void invalidBackupSummaryDoesNotPresentUncomputedImportCounts() {
        var report = JsonParser.parseString("""
            {"invalid_count":2,"invalid_entries":[{"kind":"record","index":3,"id":"broken","reason":"Invalid code"}]}
            """).getAsJsonObject();
        String summary = CodeMoriPanel.importSummary(report);
        assertTrue(summary.contains("2 项无效数据")); assertTrue(summary.contains("record[3] broken"));
        assertTrue(summary.contains("未导入")); assertFalse(summary.contains("新增资料：0"));
    }
    @Test void emptyResultTextReportsEffectiveFilters() {
        var filter = JsonParser.parseString("""
            {"workspace_id":"w1","kind":"snippet","tag":"支付","starred":true,"demo":false}
            """).getAsJsonObject();
        assertTrue(CodeMoriPanel.emptyResultText(filter, false).contains("个人资料 · 当前项目 · 代码片段 · 标签：支付 · 仅个人收藏"));
        filter.remove("tag"); filter.remove("workspace_id");
        assertTrue(CodeMoriPanel.emptyResultText(filter, true).contains("个人资料 · 全部项目 · 源码注释"));
        assertFalse(CodeMoriPanel.emptyResultText(filter, true).contains("标签："));
    }
    @Test void tagAndPathLabelsNeverInterpretUserTextAsHtml() {
        String text = "<html><b>literal tag</b>";
        var renderer = new LiteralTextRenderer();
        var label = (javax.swing.JLabel)renderer.getListCellRendererComponent(new javax.swing.JList<>(), text, 0, false, false);
        assertEquals(text, label.getText());
        assertNull(label.getClientProperty(javax.swing.plaf.basic.BasicHTML.propertyKey));
        var path = LiteralTextRenderer.label(text);
        assertEquals(text, path.getText());
        assertNull(path.getClientProperty(javax.swing.plaf.basic.BasicHTML.propertyKey));
    }
}
