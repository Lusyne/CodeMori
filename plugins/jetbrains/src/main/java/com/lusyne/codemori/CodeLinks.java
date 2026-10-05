package com.lusyne.codemori;

import com.google.gson.JsonObject;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.application.ModalityState;
import com.intellij.openapi.fileEditor.OpenFileDescriptor;
import com.intellij.openapi.progress.ProgressIndicator;
import com.intellij.openapi.progress.Task;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.project.ProjectManager;
import com.intellij.openapi.ui.popup.JBPopupFactory;
import com.intellij.openapi.vfs.LocalFileSystem;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import org.jetbrains.annotations.NotNull;

/** Only core-validated local files are passed to the editor; links never open projects or run commands. */
final class CodeLinks {
    record ProjectRoot(Project project, String root) {}
    private record Target(Project project, String root, String path, int line) { @Override public String toString() { return root; } }
    static List<ProjectRoot> openProjects() {
        return Arrays.stream(ProjectManager.getInstance().getOpenProjects())
            .filter(p -> !p.isDisposed() && p.getBasePath() != null)
            .map(p -> new ProjectRoot(p, p.getBasePath())).toList();
    }
    static CompletableFuture<String> receive(CoreClient client, List<ProjectRoot> projects, String url) {
        CompletableFuture<String> result = new CompletableFuture<>();
        ModalityState modality = ModalityState.current();
        new Task.Backgroundable(null, "打开 CodeMori 代码位置", true) {
            @Override public void run(@NotNull ProgressIndicator indicator) {
                try {
                    JsonObject parse = CoreClient.request("code_link_parse"); parse.addProperty("url", url); client.call(parse, indicator);
                    List<Target> matches = new ArrayList<>(); List<String> failures = new ArrayList<>();
                    for (ProjectRoot project : projects) {
                        indicator.checkCanceled();
                        if (project.project().isDisposed()) continue;
                        JsonObject request = CoreClient.request("code_link_resolve"); request.addProperty("url", url); request.addProperty("root", project.root());
                        try {
                            var response = client.call(request, indicator).getAsJsonObject().get("target");
                            if (!response.isJsonNull()) {
                                JsonObject value = response.getAsJsonObject();
                                matches.add(new Target(project.project(), value.get("root").getAsString(), value.get("path").getAsString(), value.get("line").getAsInt()));
                            }
                        } catch (com.intellij.openapi.progress.ProcessCanceledException error) { throw error; }
                        catch (Exception error) { failures.add(error.getMessage()); }
                    }
                    indicator.checkCanceled();
                    ApplicationManager.getApplication().invokeLater(() -> {
                        try {
                            if (matches.isEmpty()) {
                                result.complete(failures.isEmpty() ? "未找到对应项目。请先打开已拉取 .codemori/project.json 的本地工程，再使用 CodeMori: 打开代码位置链接。" : String.join("\n", failures)); return;
                            }
                            if (matches.size() == 1) openTarget(matches.getFirst(), result);
                            else JBPopupFactory.getInstance().createPopupChooserBuilder(matches)
                                .setRenderer(new LiteralTextRenderer()).setTitle("选择要打开的项目克隆")
                                .setItemChosenCallback(target -> openTarget(target, result))
                                .setCancelCallback(() -> { result.complete(null); return true; })
                                .createPopup().showCenteredInCurrentWindow(matches.getFirst().project());
                        } catch (Exception error) { result.complete(error.getMessage()); }
                    }, modality);
                } catch (Exception error) { result.complete(error.getMessage() == null ? "代码链接无法打开。" : error.getMessage()); }
            }
            @Override public void onCancel() { result.complete("操作已取消。"); }
        }.queue();
        return result;
    }
    private static void openTarget(Target target, CompletableFuture<String> result) {
        try {
            if (target.project().isDisposed()) { result.complete("目标项目已关闭。"); return; }
            var file = LocalFileSystem.getInstance().refreshAndFindFileByNioFile(Path.of(target.path()));
            if (file == null || file.isDirectory()) { result.complete("代码文件已移动或不存在，请修复文档链接。"); return; }
            new OpenFileDescriptor(target.project(), file, Math.max(0, target.line() - 1), 0).navigate(true);
            result.complete(null);
        } catch (Exception error) { result.complete(error.getMessage()); }
    }
    private CodeLinks() {}
}
