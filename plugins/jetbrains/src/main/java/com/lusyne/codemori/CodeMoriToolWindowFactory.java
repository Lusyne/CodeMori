package com.lusyne.codemori;

import com.intellij.openapi.project.Project;
import com.intellij.openapi.wm.ToolWindow;
import com.intellij.openapi.wm.ToolWindowFactory;
import com.intellij.openapi.wm.ToolWindowManager;
import com.intellij.ui.content.ContentFactory;
import java.util.function.Consumer;
import org.jetbrains.annotations.NotNull;

public final class CodeMoriToolWindowFactory implements ToolWindowFactory {
    @Override public void createToolWindowContent(@NotNull Project project, @NotNull ToolWindow window) {
        CodeMoriPanel panel = new CodeMoriPanel(project);
        var content = ContentFactory.getInstance().createContent(panel, "", false);
        content.setDisposer(panel);
        window.getContentManager().addContent(content);
    }
    static void show(Project project, Consumer<CodeMoriPanel> action) {
        ToolWindow window = ToolWindowManager.getInstance(project).getToolWindow("CodeMori");
        if (window != null) window.activate(() -> {
            var content = window.getContentManager().getContent(0);
            if (content != null && content.getComponent() instanceof CodeMoriPanel panel) action.accept(panel);
        });
    }
}
