package com.lusyne.codemori;
import com.intellij.openapi.actionSystem.*;
import org.jetbrains.annotations.NotNull;
public final class IdentityAction extends AnAction {
    @Override public void actionPerformed(@NotNull AnActionEvent event) {
        var project = event.getProject(); if (project == null) return;
        
        CodeMoriToolWindowFactory.show(project, CodeMoriPanel::configureIdentity);
    }
    @Override public void update(@NotNull AnActionEvent event) { event.getPresentation().setEnabled(event.getProject() != null); }
    @Override public @NotNull ActionUpdateThread getActionUpdateThread() { return ActionUpdateThread.EDT; }
}
