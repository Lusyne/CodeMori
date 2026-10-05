package com.lusyne.codemori;

import com.intellij.codeInsight.daemon.DaemonCodeAnalyzer;
import com.intellij.openapi.fileChooser.FileSaverDescriptor;
import com.intellij.openapi.project.Project;
import java.lang.reflect.Constructor;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;

/** Select public API signatures once, using legacy forms only where the new forms do not exist. */
final class PlatformApi {
    static final Constructor<FileSaverDescriptor> SAVE_DESCRIPTOR = saveConstructor();
    static final Method RESTART = restartMethod();

    private PlatformApi() {}

    private static Constructor<FileSaverDescriptor> saveConstructor() {
        try {
            try { return FileSaverDescriptor.class.getConstructor(String.class, String.class, String.class); }
            catch (NoSuchMethodException oldPlatform) {
                return FileSaverDescriptor.class.getConstructor(String.class, String.class, String[].class);
            }
        } catch (NoSuchMethodException error) { throw new ExceptionInInitializerError(error); }
    }

    private static Method restartMethod() {
        try {
            try { return DaemonCodeAnalyzer.class.getMethod("restart", Object.class); }
            catch (NoSuchMethodException oldPlatform) { return DaemonCodeAnalyzer.class.getMethod("restart"); }
        } catch (NoSuchMethodException error) { throw new ExceptionInInitializerError(error); }
    }

    static FileSaverDescriptor backupDescriptor() {
        Object extension = SAVE_DESCRIPTOR.getParameterTypes()[2] == String.class ? "json" : new String[]{"json"};
        try {
            return SAVE_DESCRIPTOR.newInstance("导出 CodeMori 备份", "包含片段、标签、文档摘要、工作区和文件关联", extension);
        } catch (ReflectiveOperationException error) { throw invocationFailure(error); }
    }

    static void restartDaemon(Project project) {
        Object[] arguments = RESTART.getParameterCount() == 0 ? new Object[0] : new Object[]{"CodeMori document hints"};
        try { RESTART.invoke(DaemonCodeAnalyzer.getInstance(project), arguments); }
        catch (ReflectiveOperationException error) { throw invocationFailure(error); }
    }

    private static RuntimeException invocationFailure(ReflectiveOperationException error) {
        Throwable cause = error instanceof InvocationTargetException ? error.getCause() : error;
        if (cause instanceof RuntimeException failure) return failure;
        if (cause instanceof Error failure) throw failure;
        return new IllegalStateException("CodeMori platform API call failed", cause);
    }
}
