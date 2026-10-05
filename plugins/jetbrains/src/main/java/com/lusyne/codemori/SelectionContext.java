package com.lusyne.codemori;

import com.intellij.openapi.editor.Editor;
import com.intellij.openapi.fileEditor.FileDocumentManager;
import com.intellij.openapi.fileEditor.FileEditorManager;
import com.intellij.openapi.project.Project;
import java.nio.file.Path;

/** Capture on EDT before asynchronous work, so an editor switch cannot change the target. */
public record SelectionContext(String root, String path, Integer line, String code) {
    public static SelectionContext capture(Project project) {
        return capture(project, FileEditorManager.getInstance(project).getSelectedTextEditor());
    }
    public static SelectionContext capture(Project project, Editor editor) {
        String root = project.getBasePath();
        String path = null;
        Integer line = null;
        String code = "";
        if (editor != null) {
            code = editor.getSelectionModel().getSelectedText();
            if (code == null) code = "";
            line = editor.getDocument().getLineNumber(editor.getSelectionModel().getSelectionStart()) + 1;
            var file = FileDocumentManager.getInstance().getFile(editor.getDocument());
            if (root != null && file != null && file.isInLocalFileSystem()) {
                Path base = Path.of(root).toAbsolutePath().normalize();
                Path target = Path.of(file.getPath()).toAbsolutePath().normalize();
                if (target.startsWith(base)) path = base.relativize(target).toString().replace('\\', '/');
            }
        }
        return new SelectionContext(root, path, line, code);
    }
    public boolean hasFile() { return root != null && path != null && !path.isBlank(); }
}
