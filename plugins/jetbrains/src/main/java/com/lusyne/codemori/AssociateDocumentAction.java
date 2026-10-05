package com.lusyne.codemori;

import com.intellij.openapi.actionSystem.*;
import org.jetbrains.annotations.NotNull;

public final class AssociateDocumentAction extends AnAction {
    @Override public void actionPerformed(@NotNull AnActionEvent event) {
        if (event.getProject() == null) return;
        var context = SelectionContext.capture(event.getProject(), event.getData(CommonDataKeys.EDITOR));
        CodeMoriToolWindowFactory.show(event.getProject(), panel -> panel.associate(context));
    }
    @Override public void update(@NotNull AnActionEvent event) { event.getPresentation().setEnabled(event.getProject() != null); }
    @Override public @NotNull ActionUpdateThread getActionUpdateThread() { return ActionUpdateThread.BGT; }
}
