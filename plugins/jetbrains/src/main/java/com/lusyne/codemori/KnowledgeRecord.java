package com.lusyne.codemori;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.List;

/** A validated projection for rendering; mutations send the observed revision to Rust. */
public record KnowledgeRecord(JsonObject raw) {
    public static KnowledgeRecord from(JsonElement value) {
        if (!value.isJsonObject()) throw new IllegalArgumentException("Invalid knowledge record.");
        JsonObject record = value.getAsJsonObject();
        requiredString(record, "id");
        if (!record.has("revision") || record.get("revision").getAsLong() < 1) throw new IllegalArgumentException("Missing record revision.");
        JsonObject input = record.getAsJsonObject("input");
        if (input == null) throw new IllegalArgumentException("Missing record content.");
        String kind = requiredString(input, "kind");
        if (!kind.equals("snippet") && !kind.equals("document")) throw new IllegalArgumentException("Unknown record kind.");
        for (String name : List.of("title", "content", "description", "language")) requiredString(input, name);
        if (!input.has("tags") || !input.get("tags").isJsonArray()) throw new IllegalArgumentException("Missing tags.");
        for (JsonElement tag : input.getAsJsonArray("tags")) {
            if (!tag.isJsonPrimitive() || !tag.getAsJsonPrimitive().isString()) throw new IllegalArgumentException("Invalid tag.");
        }
        if (!input.has("starred") || !input.get("starred").isJsonPrimitive() || !input.get("starred").getAsJsonPrimitive().isBoolean()) throw new IllegalArgumentException("Missing favorite status.");
        if (kind.equals("document")) requiredString(input, "url");
        if (record.has("scope")) {
            String scope = requiredString(record, "scope");
            if (!scope.equals("personal") && !scope.equals("project")) throw new IllegalArgumentException("Invalid record scope.");
            if (scope.equals("project")) { requiredString(record, "project_root"); requiredString(record, "project_version"); }
        }
        for (String key : List.of("created_by", "updated_by")) {
            if (record.has(key) && !record.get(key).isJsonNull()) {
                JsonObject author = record.getAsJsonObject(key); requiredString(author, "id"); requiredString(author, "display_name");
            }
        }
        return new KnowledgeRecord(record.deepCopy());
    }
    public boolean shared() { return raw.has("scope") && raw.get("scope").getAsString().equals("project"); }
    public String attribution() { return shared() ? "创建者：" + author("created_by") + " · 最近修改：" + author("updated_by") : ""; }
    private String author(String key) { return raw.has(key) && !raw.get(key).isJsonNull() ? raw.getAsJsonObject(key).get("display_name").getAsString() : "历史未署名"; }
    public String scopeLabel() { return shared() ? "项目共享" : "个人"; }
    public String projectRoot() { return shared() ? raw.get("project_root").getAsString() : null; }
    public String projectVersion() { return shared() ? raw.get("project_version").getAsString() : null; }
    public String key() { JsonArray key = new JsonArray(); key.add(shared() ? "project" : "personal"); key.add(projectRoot()); key.add(id()); return key.toString(); }
    public boolean localDocument() { return document() && (field("url").startsWith("file:") || (shared() && field("url").startsWith("./"))); }
    public String id() { return raw.get("id").getAsString(); }
    public long revision() { return raw.get("revision").getAsLong(); }
    public JsonObject input() { return raw.getAsJsonObject("input").deepCopy(); }
    public String field(String name) { return raw.getAsJsonObject("input").get(name).getAsString(); }
    public boolean document() { return field("kind").equals("document"); }
    public boolean starred() { return raw.getAsJsonObject("input").get("starred").getAsBoolean(); }
    public List<String> tags() { return strings(raw.getAsJsonObject("input").getAsJsonArray("tags")); }
    public static List<String> strings(JsonArray values) {
        List<String> result = new ArrayList<>();
        for (JsonElement value : values) result.add(value.getAsString());
        return result;
    }
    private static String requiredString(JsonObject value, String name) {
        JsonElement field = value.get(name);
        if (field == null || !field.isJsonPrimitive() || !field.getAsJsonPrimitive().isString()) throw new IllegalArgumentException("Missing string: " + name);
        return field.getAsString();
    }
    public static String escape(String text) {
        return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;").replace("'", "&#39;");
    }
    public String linkedDocumentHtml(int width) {
        if (!document()) throw new IllegalStateException("Only documents appear in the file association list");
        String summary = field("description").isBlank() ? "未填写摘要" : field("description");
        return "<html><div style='width:" + Math.max(160, width) + "px'><b>" + escape(scopeLabel() + " · " + field("title")) +
            "</b><br>" + (shared() ? escape(attribution()) + "<br>" : "") + "<small>来源：" + escape(field("url")) + "</small><br>" + escape(summary).replace("\n", "<br>") + "</div></html>";
    }
    @Override public String toString() { return (document() ? "文档 · " : starred() ? "★ 片段 · " : "片段 · ") + field("title"); }
}
