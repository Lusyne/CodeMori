package com.lusyne.codemori;

import com.google.gson.JsonObject;
import com.intellij.ide.util.PropertiesComponent;
import com.intellij.openapi.Disposable;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.application.ModalityState;
import com.intellij.openapi.components.Service;
import com.intellij.openapi.progress.ProgressIndicator;
import com.intellij.openapi.progress.Task;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.vfs.VirtualFileManager;
import com.intellij.openapi.vfs.newvfs.BulkFileListener;
import com.intellij.openapi.vfs.newvfs.events.VFileEvent;
import com.intellij.psi.PsiFile;
import java.nio.file.Path;
import java.util.List;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import org.jetbrains.annotations.NotNull;

/** Background context cache. PSI marker callbacks never execute a CLI synchronously. */
@Service(Service.Level.PROJECT)
public final class DocumentHints implements Disposable {
    private final Project project;
    private final String rootOverride;
    private final CoreClient clientOverride;
    private final ConcurrentHashMap<String,String> cache = new ConcurrentHashMap<>();
    private final Set<String> pending = ConcurrentHashMap.newKeySet();
    private volatile int generation;
    private final javax.swing.Timer restartTimer;
    private volatile boolean disposed;
    public DocumentHints(Project project) { this(project, null, null); }
    DocumentHints(Project project, String root, CoreClient client) {
        this.project = project; this.rootOverride = root; this.clientOverride = client;
        restartTimer = new javax.swing.Timer(150, event -> { if (!disposed && !project.isDisposed()) PlatformApi.restartDaemon(project); }); restartTimer.setRepeats(false);
        project.getMessageBus().connect(this).subscribe(com.intellij.openapi.fileEditor.FileEditorManagerListener.FILE_EDITOR_MANAGER, new com.intellij.openapi.fileEditor.FileEditorManagerListener() {
            @Override public void selectionChanged(@NotNull com.intellij.openapi.fileEditor.FileEditorManagerEvent event) { invalidate(); }
        });
        ApplicationManager.getApplication().getMessageBus().connect(this).subscribe(VirtualFileManager.VFS_CHANGES, new BulkFileListener() {
            @Override public void after(@NotNull List<? extends VFileEvent> events) {
                String root = rootOverride == null ? project.getBasePath() : rootOverride;
                if (root != null && events.stream().anyMatch(e -> e.getPath().startsWith(root + "/"))) invalidate();
            }
        });
    }
    public static DocumentHints get(Project project) { return project.getService(DocumentHints.class); }
    public boolean enabled() { return PropertiesComponent.getInstance(project).getBoolean("codemori.documentHints.enabled", true); }
    public void toggle() { PropertiesComponent.getInstance(project).setValue("codemori.documentHints.enabled", !enabled(), true); invalidate(); }
    public void invalidate() {
        generation++; cache.clear();
        restart();
    }
    private void restart() { ApplicationManager.getApplication().invokeLater(() -> { if (!disposed && !project.isDisposed()) restartTimer.restart(); }, ModalityState.nonModal()); }
    String summary(PsiFile file) {
        if (clientOverride == null && ApplicationManager.getApplication().isUnitTestMode() && System.getProperty("codemori.testDataDir") == null) return null;
        if (!enabled() || disposed || file.getVirtualFile() == null || !file.getVirtualFile().isInLocalFileSystem() || (rootOverride == null && project.getBasePath() == null)) return null;
        String root = rootOverride == null ? project.getBasePath() : rootOverride, absolute = file.getVirtualFile().getPath();
        Path base = Path.of(root), target = Path.of(absolute); if (!target.startsWith(base)) return null;
        String existing = cache.get(absolute); if (existing != null) return existing.isEmpty() ? null : existing;
        if (!pending.add(absolute)) return null;
        int ticket = generation; String relative = base.relativize(target).toString().replace('\\', '/');
        new Task.Backgroundable(project, "CodeMori 文档提示", true) {
            @Override public void run(@NotNull ProgressIndicator indicator) {
                String text = "";
                try {
                    CoreClient client = clientOverride == null ? CoreClient.bundled() : clientOverride;
                    JsonObject register = CoreClient.request("workspace_register"); register.addProperty("root", root);
                    JsonObject workspace = client.call(register, indicator).getAsJsonObject();
                    JsonObject request = CoreClient.request("library_file_documents"); request.addProperty("root", workspace.get("root").getAsString()); request.addProperty("workspace_id", workspace.get("id").getAsString()); request.addProperty("path", relative);
                    JsonObject data = client.call(request, indicator).getAsJsonObject();
                    int count = data.getAsJsonArray("records").size(), reviews = 0;
                    for (var entry : data.getAsJsonArray("entries")) if (entry.getAsJsonObject().getAsJsonObject("review_state").get("status").getAsString().equals("needs_review")) reviews++;
                    if (count > 0) text = "CodeMori · " + count + " 篇关联文档" + (reviews > 0 ? " · " + reviews + " 项待复核" : "") + "（点击查看，基于已保存代码）";
                } catch (Exception ignored) { /* The full panel displays core errors; hints remain nonblocking. */ }
                finally { pending.remove(absolute); }
                if (disposed || project.isDisposed()) return;
                if (ticket != generation) { restart(); return; }
                if (cache.size() >= 128) cache.clear(); cache.put(absolute, text);
                restart();
            }
        }.queue();
        return null;
    }
    @Override public void dispose() { disposed = true; generation++; cache.clear(); restartTimer.stop(); }
}
