package com.lusyne.codemori;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.math.BigDecimal;

/** The only decoder for the versioned CLI boundary. */
public record CliResponse(String version, String dataDirectory, String databasePath, int schemaVersion) {
    public static CliResponse parse(String text) {
        JsonObject data = object(data(text), "data");
        return new CliResponse(string(data, "version"), string(data, "data_dir"),
                string(data, "database_path"), integer(data, "schema_version"));
    }

    public static JsonElement data(String text) {
        JsonObject root = object(JsonParser.parseString(text), "response");
        if (integer(root, "protocol_version") != 1) {
            throw new IllegalArgumentException("Unsupported CodeMori CLI protocol; update the plugin.");
        }
        JsonElement ok = root.get("ok");
        if (ok == null || !ok.isJsonPrimitive() || !ok.getAsJsonPrimitive().isBoolean()) {
            throw new IllegalArgumentException("Missing boolean status in CLI response.");
        }
        if (!ok.getAsBoolean()) {
            JsonObject error = object(root.get("error"), "error");
            throw new IllegalArgumentException(string(error, "code") + ": " + string(error, "message"));
        }
        JsonElement data = root.get("data");
        if (data == null || data.isJsonNull()) throw new IllegalArgumentException("Missing CLI response data.");
        return data;
    }

    private static JsonObject object(JsonElement value, String name) {
        if (value == null || !value.isJsonObject()) {
            throw new IllegalArgumentException("Missing object: " + name);
        }
        return value.getAsJsonObject();
    }

    private static String string(JsonObject object, String name) {
        JsonElement value = object.get(name);
        if (value == null || !value.isJsonPrimitive() || !value.getAsJsonPrimitive().isString()
                || value.getAsString().isBlank()) {
            throw new IllegalArgumentException("Missing nonempty string: " + name);
        }
        return value.getAsString();
    }

    private static int integer(JsonObject object, String name) {
        JsonElement value = object.get(name);
        if (value == null || !value.isJsonPrimitive() || !value.getAsJsonPrimitive().isNumber()) {
            throw new IllegalArgumentException("Missing integer: " + name);
        }
        try {
            return new BigDecimal(value.getAsString()).intValueExact();
        } catch (ArithmeticException | NumberFormatException exception) {
            throw new IllegalArgumentException("Invalid integer: " + name, exception);
        }
    }
}
