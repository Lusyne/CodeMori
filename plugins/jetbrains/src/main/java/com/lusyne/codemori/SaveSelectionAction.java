package com.lusyne.codemori;

import com.intellij.openapi.actionSystem.*;
import org.jetbrains.annotations.NotNull;

public final class SaveSelectionAction extends AnAction {
    @Override public void actionPerformed(@NotNull AnActionEvent event) {
        var project = event.getProject();
        if (project == null) return;
        SelectionContext context = SelectionContext.capture(project, event.getData(CommonDataKeys.EDITOR));
        CodeMoriToolWindowFactory.show(project, panel -> panel.capture(context));
    }
    @Override public void update(@NotNull AnActionEvent event) {
        var editor = event.getData(CommonDataKeys.EDITOR);
        event.getPresentation().setEnabledAndVisible(event.getProject() != null && editor != null && editor.getSelectionModel().hasSelection());
    }
    @Override public @NotNull ActionUpdateThread getActionUpdateThread() { return ActionUpdateThread.EDT; }
}
