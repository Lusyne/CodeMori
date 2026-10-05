package com.lusyne.codemori;

import com.intellij.openapi.application.JBProtocolCommand;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Future;
import java.util.function.Supplier;

public final class CodeLinkProtocol extends JBProtocolCommand {
    private final Supplier<CoreClient> client;
    private final Supplier<List<CodeLinks.ProjectRoot>> projects;
    public CodeLinkProtocol() { this(CoreClient::bundled, CodeLinks::openProjects); }
    CodeLinkProtocol(Supplier<CoreClient> client, Supplier<List<CodeLinks.ProjectRoot>> projects) {
        super("codemori"); this.client = client; this.projects = projects;
    }
    @Override public Future<String> perform(String target, Map<String, String> parameters, String fragment) {
        if (!"open".equals(target) || fragment != null) return CompletableFuture.completedFuture("不支持的 CodeMori 代码链接。");
        StringBuilder url = new StringBuilder("jetbrains://idea/codemori/open?");
        for (var entry : parameters.entrySet()) {
            if (entry.getValue() == null) return CompletableFuture.completedFuture("代码链接参数无效。");
            url.append(URLEncoder.encode(entry.getKey(), StandardCharsets.UTF_8)).append('=')
                .append(URLEncoder.encode(entry.getValue(), StandardCharsets.UTF_8)).append('&');
        }
        try { return CodeLinks.receive(client.get(), projects.get(), url.toString().replaceFirst("&$", "")); }
        catch (Exception error) { return CompletableFuture.completedFuture(error.getMessage()); }
    }
}
