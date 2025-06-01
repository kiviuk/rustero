#[derive(Debug, Default, Clone)]
pub struct ScrollableParagraphState {
    pub content: String, // Or ratatui::text::Text<'a> for styled text
    pub scroll_offset_vertical: u16,
    pub scroll_offset_horizontal: u16, // If you want horizontal scrolling too
                                       // You might also store:
                                       // - total_content_lines: usize (if you can calculate/estimate it)
                                       // - panel_height: u16 (from the layout, to cap scrolling)
}

impl ScrollableParagraphState {
    pub fn new(content: String) -> Self {
        Self {
            content,
            scroll_offset_vertical: 0,
            scroll_offset_horizontal: 0,
            // ..
        }
    }

    pub fn set_content(&mut self, content: String) {
        self.content = content;
        self.scroll_offset_vertical = 0; // Reset scroll when content changes
        self.scroll_offset_horizontal = 0;
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset_vertical = self.scroll_offset_vertical.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: u16 /*, max_scroll: u16 */) {
        // If you had max_scroll:
        // self.scroll_offset_vertical = self.scroll_offset_vertical.saturating_add(amount).min(max_scroll);
        self.scroll_offset_vertical = self.scroll_offset_vertical.saturating_add(amount);
    }
}
