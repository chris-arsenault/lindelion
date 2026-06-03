//! Pure UI layout math shared by fixed-format Galad tools.
//!
//! The contract is: leaf metrics describe typography, row heights, visible row counts, borders,
//! scrollbars, and padding; region sizes are derived from those metrics. The Vizia views should not
//! carry independent width/height guesses for the same panel.

#[derive(Clone, Copy, Debug)]
pub(crate) struct ToolFrameMetrics {
    pub padding: f32,
    pub border_width: f32,
    pub scrollbar_width: f32,
    pub vertical_gap: f32,
    pub header_button_height: f32,
    pub divider_height: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BrowserTreeMetrics {
    pub vendor_font_size: f32,
    pub plugin_font_size: f32,
    pub count_font_size: f32,
    pub vendor_row_height: f32,
    pub plugin_row_height: f32,
    pub row_padding_left: f32,
    pub row_padding_right: f32,
    pub caret_size: f32,
    pub plugin_indent: f32,
    pub item_gap: f32,
    pub name_columns: u16,
    pub count_columns: u16,
    pub average_glyph_width_em: f32,
}

impl BrowserTreeMetrics {
    pub fn top_inset(self) -> f32 {
        centered_line_inset(self.vendor_row_height, self.vendor_font_size).ceil()
    }

    pub fn name_column_width(self) -> f32 {
        text_columns_width(
            self.plugin_font_size,
            self.name_columns,
            self.average_glyph_width_em,
        )
    }

    pub fn count_column_width(self) -> f32 {
        text_columns_width(
            self.count_font_size,
            self.count_columns,
            self.average_glyph_width_em,
        )
    }

    pub fn vendor_row_width(self) -> f32 {
        self.row_padding_left
            + self.caret_size
            + self.item_gap
            + self.name_column_width()
            + self.item_gap
            + self.count_column_width()
            + self.row_padding_right
    }

    pub fn plugin_row_width(self) -> f32 {
        self.row_padding_left
            + self.plugin_indent
            + self.name_column_width()
            + self.row_padding_right
    }

    pub fn row_background_width(self) -> f32 {
        self.vendor_row_width().max(self.plugin_row_width()).ceil()
    }

    pub fn viewport_inner_height(self, vendor_rows: u16, plugin_rows: u16) -> f32 {
        self.top_inset()
            + (f32::from(vendor_rows) * self.vendor_row_height)
            + (f32::from(plugin_rows) * self.plugin_row_height)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ToolPanelMetrics {
    pub frame: ToolFrameMetrics,
    pub tree: BrowserTreeMetrics,
    pub visible_vendor_rows: u16,
    pub visible_plugin_rows: u16,
}

impl ToolPanelMetrics {
    pub fn header_height(self) -> f32 {
        self.frame.header_button_height + (self.frame.border_width * 2.0)
    }

    pub fn row_stack_width(self) -> f32 {
        self.tree.row_background_width()
    }

    pub fn viewport_width(self) -> f32 {
        self.row_stack_width() + (self.frame.border_width * 2.0) + self.frame.scrollbar_width
    }

    pub fn viewport_height(self) -> f32 {
        self.tree
            .viewport_inner_height(self.visible_vendor_rows, self.visible_plugin_rows)
            + (self.frame.border_width * 2.0)
    }

    pub fn outer_width(self) -> f32 {
        (self.frame.padding * 2.0) + self.viewport_width()
    }

    pub fn outer_height(self) -> f32 {
        (self.frame.padding * 2.0)
            + self.header_height()
            + self.frame.divider_height
            + self.viewport_height()
            + (self.frame.vertical_gap * 2.0)
    }
}

pub(crate) const ADD_PLUGIN_TREE: BrowserTreeMetrics = BrowserTreeMetrics {
    vendor_font_size: 10.0,
    plugin_font_size: 10.0,
    count_font_size: 9.0,
    vendor_row_height: 22.0,
    plugin_row_height: 21.0,
    row_padding_left: 5.0,
    row_padding_right: 14.0,
    caret_size: 12.0,
    plugin_indent: 14.0,
    item_gap: 4.0,
    name_columns: 48,
    count_columns: 5,
    average_glyph_width_em: 0.58,
};

pub(crate) const ADD_PLUGIN_PANEL: ToolPanelMetrics = ToolPanelMetrics {
    frame: ToolFrameMetrics {
        padding: 10.0,
        border_width: 1.0,
        scrollbar_width: 7.0,
        vertical_gap: 7.0,
        header_button_height: 22.0,
        divider_height: 1.0,
    },
    tree: ADD_PLUGIN_TREE,
    visible_vendor_rows: 2,
    visible_plugin_rows: 16,
};

fn centered_line_inset(row_height: f32, font_size: f32) -> f32 {
    ((row_height - font_size) * 0.5).max(0.0)
}

fn text_columns_width(font_size: f32, columns: u16, average_glyph_width_em: f32) -> f32 {
    (font_size * average_glyph_width_em * f32::from(columns)).ceil()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 0.001;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPSILON,
            "actual={actual} expected={expected}"
        );
    }

    #[test]
    fn add_plugin_panel_size_is_sum_of_regions() {
        let panel = ADD_PLUGIN_PANEL;

        assert_close(
            panel.viewport_width(),
            panel.row_stack_width()
                + (panel.frame.border_width * 2.0)
                + panel.frame.scrollbar_width,
        );
        assert_close(
            panel.outer_width(),
            (panel.frame.padding * 2.0) + panel.viewport_width(),
        );
        assert_close(
            panel.outer_height(),
            (panel.frame.padding * 2.0)
                + panel.header_height()
                + panel.frame.divider_height
                + panel.viewport_height()
                + (panel.frame.vertical_gap * 2.0),
        );
    }

    #[test]
    fn tree_rows_reserve_text_gutter_inside_full_background() {
        let tree = ADD_PLUGIN_TREE;
        let vendor_text_end = tree.row_padding_left
            + tree.caret_size
            + tree.item_gap
            + tree.name_column_width()
            + tree.item_gap
            + tree.count_column_width();
        let vendor_reserved = vendor_text_end + tree.row_padding_right;

        assert!(tree.row_background_width() >= vendor_reserved);
        assert!(tree.row_background_width() >= tree.plugin_row_width());
    }

    #[test]
    fn first_tree_row_inset_is_derived_from_label_centering() {
        let tree = ADD_PLUGIN_TREE;

        assert_close(
            tree.top_inset(),
            ((tree.vendor_row_height - tree.vendor_font_size) * 0.5).ceil(),
        );
    }
}
