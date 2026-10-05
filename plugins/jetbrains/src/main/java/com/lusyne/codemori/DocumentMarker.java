package com.lusyne.codemori;
import com.intellij.codeInsight.daemon.LineMarkerInfo;
import com.intellij.codeInsight.daemon.LineMarkerProviderDescriptor;
import com.intellij.icons.AllIcons;
import com.intellij.openapi.editor.markup.GutterIconRenderer;
import com.intellij.psi.PsiElement;
import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Nullable;

public final class DocumentMarker extends LineMarkerProviderDescriptor {
    private final java.util.function.Function<com.intellij.psi.PsiFile,String> summaries;
    public DocumentMarker() { this(file -> DocumentHints.get(file.getProject()).summary(file)); }
    DocumentMarker(java.util.function.Function<com.intellij.psi.PsiFile,String> summaries) { this.summaries = summaries; }
    @Override public String getName() { return "CodeMori 关联文档"; }
    @Override public @Nullable LineMarkerInfo<?> getLineMarkerInfo(@NotNull PsiElement element) {
        if (element.getFirstChild() != null || element.getTextRange().getStartOffset() != 0 || element.getTextLength() == 0 || element.getContainingFile() == null) return null;
        String summary = summaries.apply(element.getContainingFile());
        if (summary == null) return null;
        return new LineMarkerInfo<>(element, element.getTextRange(), AllIcons.FileTypes.Text, ignored -> summary,
            (event, target) -> CodeMoriToolWindowFactory.show(target.getProject(), CodeMoriPanel::focusSearch), GutterIconRenderer.Alignment.LEFT, () -> summary);
    }
}
