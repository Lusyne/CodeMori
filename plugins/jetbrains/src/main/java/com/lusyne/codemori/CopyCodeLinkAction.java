package com.lusyne.codemori;
import com.intellij.openapi.actionSystem.*;
import org.jetbrains.annotations.NotNull;
public final class CopyCodeLinkAction extends AnAction {
    @Override public void actionPerformed(@NotNull AnActionEvent event) {
        var project = event.getProject(); if (project == null) return;
        SelectionContext selection = SelectionContext.capture(project, event.getData(CommonDataKeys.EDITOR));
        CodeMoriToolWindowFactory.show(project, panel -> panel.copyCodeLink(selection));
    }
    @Override public void update(@NotNull AnActionEvent event) { event.getPresentation().setEnabled(event.getProject() != null); }
    @Override public @NotNull ActionUpdateThread getActionUpdateThread() { return ActionUpdateThread.EDT; }
}
