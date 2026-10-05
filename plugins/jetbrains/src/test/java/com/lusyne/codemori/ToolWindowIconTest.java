package com.lusyne.codemori;

import com.intellij.openapi.util.IconLoader;
import com.intellij.testFramework.fixtures.BasePlatformTestCase;
import javax.xml.parsers.DocumentBuilderFactory;
import org.w3c.dom.Element;

public final class ToolWindowIconTest extends BasePlatformTestCase {
    public void testRegisteredToolWindowHasLoadableLightAndDarkIcons() throws Exception {
        // Headless IDE tests otherwise return a 1x1 placeholder without loading the SVG.
        IconLoader.activate();
        try (var input = CodeMoriToolWindowFactory.class.getResourceAsStream("/META-INF/plugin.xml")) {
            assertNotNull(input);
            var descriptor = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(input);
            var windows = descriptor.getElementsByTagName("toolWindow");
            assertEquals(1, windows.getLength());
            var window = (Element) windows.item(0);
            assertEquals("CodeMori", window.getAttribute("id"));
            assertEquals("right", window.getAttribute("anchor"));
            assertEquals(CodeMoriToolWindowFactory.class.getName(), window.getAttribute("factoryClass"));
            String resource = window.getAttribute("icon");
            for (String icon : new String[]{resource, resource.replace(".svg", "_dark.svg")}) {
                assertNotNull(CodeMoriToolWindowFactory.class.getResource(icon));
                var loaded = IconLoader.getIcon(icon, CodeMoriToolWindowFactory.class);
                assertEquals(13, loaded.getIconWidth()); assertEquals(13, loaded.getIconHeight());
            }
        }
    }
}
