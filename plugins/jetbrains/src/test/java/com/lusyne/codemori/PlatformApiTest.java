package com.lusyne.codemori;

import com.intellij.testFramework.fixtures.BasePlatformTestCase;

public final class PlatformApiTest extends BasePlatformTestCase {
    public void testSelectedApisAreNotDeprecatedOnTheRunningPlatform() {
        assertNull(PlatformApi.SAVE_DESCRIPTOR.getAnnotation(Deprecated.class));
        assertNull(PlatformApi.RESTART.getAnnotation(Deprecated.class));
        PlatformApi.restartDaemon(getProject());
    }

    public void testBackupDescriptorRetainsJsonFilterAndLabels() {
        var descriptor = PlatformApi.backupDescriptor();
        assertEquals("导出 CodeMori 备份", descriptor.getTitle());
        assertTrue(descriptor.getDescription().contains("文件关联"));
        assertTrue(descriptor.isFileVisible(myFixture.addFileToProject("backup.json", "{}").getVirtualFile(), false));
        assertFalse(descriptor.isFileVisible(myFixture.addFileToProject("backup.txt", "text").getVirtualFile(), false));
    }
}
