package com.lusyne.codemori;

import com.intellij.openapi.Disposable;
import com.intellij.openapi.ui.TestDialog;
import com.intellij.openapi.ui.TestDialogManager;
import com.intellij.openapi.util.Disposer;

final class TestDialogs {
    private TestDialogs() {}

    static void set(TestDialog dialog, Disposable parent) {
        var previous = TestDialogManager.setTestDialog(dialog);
        Disposer.register(parent, () -> TestDialogManager.setTestDialog(previous));
    }
}
