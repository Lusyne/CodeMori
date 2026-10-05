package com.lusyne.codemori;

import javax.swing.DefaultListCellRenderer;
import javax.swing.JLabel;

/** User tags and paths are text, including strings that start with Swing's HTML marker. */
final class LiteralTextRenderer extends DefaultListCellRenderer {
    LiteralTextRenderer() { putClientProperty("html.disable", Boolean.TRUE); }

    static JLabel label(String text) {
        JLabel label = new JLabel();
        label.putClientProperty("html.disable", Boolean.TRUE);
        label.setText(text);
        return label;
    }
}
