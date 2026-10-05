package com.lusyne.codemori;

import com.intellij.openapi.util.BuildNumber;
import javax.xml.parsers.DocumentBuilderFactory;
import org.junit.jupiter.api.Test;
import org.w3c.dom.Element;
import static org.junit.jupiter.api.Assertions.*;

final class PluginCompatibilityTest {
    @Test void descriptorAllows2024_2AndFutureBuilds() throws Exception {
        try (var input = CodeMoriToolWindowFactory.class.getResourceAsStream("/META-INF/plugin.xml")) {
            assertNotNull(input);
            var descriptor = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(input);
            var range = (Element) descriptor.getElementsByTagName("idea-version").item(0);
            assertNotNull(range);
            var minimum = BuildNumber.fromString(range.getAttribute("since-build"));
            assertNotNull(minimum);
            assertTrue(minimum.compareTo(BuildNumber.fromString("242.20224.300")) <= 0);
            assertFalse(range.hasAttribute("until-build"), "IDE upgrades must not be blocked by an upper build limit");
        }
    }
}
