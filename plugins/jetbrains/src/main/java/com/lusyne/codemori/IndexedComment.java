package com.lusyne.codemori;

import com.google.gson.JsonObject;
import java.time.Instant;
import java.time.ZoneId;
import java.time.format.DateTimeFormatter;

/** Read-only index projection; these entries are never editable knowledge records. */
public record IndexedComment(JsonObject raw) {
    public JsonObject source() { return raw.getAsJsonObject("source").deepCopy(); }
    public String location() { return raw.get("workspace_name").getAsString() + " · " + source().get("path").getAsString() + ":" + source().get("line").getAsInt(); }
    public String text() { return raw.get("text").getAsString(); }
    public String indexedAt() { return DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm:ss").withZone(ZoneId.systemDefault()).format(Instant.ofEpochMilli(raw.get("indexed_at").getAsLong())); }
    public String html() {
        StringBuilder html = new StringBuilder("<html><b>").append(KnowledgeRecord.escape(location())).append("</b><br>");
        for (var value : raw.getAsJsonArray("excerpt")) {
            var span = value.getAsJsonObject(); String text = KnowledgeRecord.escape(span.get("text").getAsString()).replace("\n", " ");
            html.append(span.get("highlight").getAsBoolean() ? "<b>" + text + "</b>" : text);
        }
        return html.append("<br>索引于 ").append(indexedAt()).append(" · 双击或 Enter 跳转</html>").toString();
    }
}
