package com.lusyne.codemori;

import com.intellij.openapi.actionSystem.*;
import org.jetbrains.annotations.NotNull;

public final class SearchAction extends AnAction {
    @Override public void actionPerformed(@NotNull AnActionEvent event) {
        if (event.getProject() != null) CodeMoriToolWindowFactory.show(event.getProject(), CodeMoriPanel::focusSearch);
    }
    @Override public void update(@NotNull AnActionEvent event) { event.getPresentation().setEnabled(event.getProject() != null); }
    @Override public @NotNull ActionUpdateThread getActionUpdateThread() { return ActionUpdateThread.BGT; }
}
