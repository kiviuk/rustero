// src/widgets/scrollable_paragraph.rs
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Default, Clone)]
pub struct ScrollableParagraphState {
    pub content: String, // Or ratatui::text::Text<'a> for styled text
    pub scroll_offset_vertical: u16,
    pub scroll_offset_horizontal: u16, // If you want horizontal scrolling too
    // You might also store:
    // - total_content_lines: usize (if you can calculate/estimate it)
    // - panel_height: u16 (from the layout, to cap scrolling)
    pub panel_height: u16, // <-- new
    pub panel_width: u16,  // Add this for wrap-aware calculations
}

impl ScrollableParagraphState {
    pub fn new(content: String) -> Self {
        Self {
            content,
            scroll_offset_vertical: 0,
            scroll_offset_horizontal: 0,
            panel_height: 0,
            panel_width: 0,
        }
    }
    fn calculate_content_height_lines(&self) -> u16 {
        // You are calculating calculate_content_height_lines() on the fly whenever max_scroll_vertical()
        // (and thus scroll_down()) is called, and also when set_dimensions() calls max_scroll_vertical().
        // This is fine, just potentially less performant if content is huge and these are called frequently,
        // but for typical show notes, it might be acceptable.
        let available_width = self.panel_width.saturating_sub(self.scroll_offset_horizontal);

        if available_width == 0 {
            return 0; // No space to render anything
        }
        let available_width_usize = available_width as usize;

        let total_rendered_lines = self.content.lines().fold(0u16, |acc, original_line| {
            let line_unicode_width: usize =
                original_line.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(0)).sum();

            let rendered_rows_for_this_line = if line_unicode_width == 0 {
                1 // An empty original line still takes up one rendered line
            } else {
                // Ceiling division: (numerator + denominator - 1) / denominator
                // How many groups of `denominator` fit into `numerator`, rounding UP:
                ((line_unicode_width + available_width_usize - 1) / available_width_usize) as u16
            };
            // eprintln!("Line: '{}', unicode_width: {}, panel_w: {}, rows: {}", original_line, line_unicode_width, self.panel_width, rendered_rows_for_this_line);
            acc.saturating_add(rendered_rows_for_this_line)
        });

        total_rendered_lines
    }

    pub fn max_scroll_vertical(&self) -> u16 {
        let total_content_height = self.calculate_content_height_lines();
        total_content_height.saturating_sub(self.panel_height)
    }
    pub fn set_content(&mut self, content: String) {
        // eprintln!("--- ScrollableParagraphState::set_content ---");
        // eprintln!("Received content (first 200 chars): {:.200}", content);
        // eprintln!("Content total original lines: {}", content.lines().count());

        self.content = content.trim().to_string();
        self.scroll_offset_vertical = 0; // Reset scroll when content changes
        self.scroll_offset_horizontal = 0;
    }

    // You'll also need a method to set the panel_width and panel_height.
    // This should be called from terminal_ui whenever the layout chunk size for show notes is known.
    pub fn set_dimensions(&mut self, width: u16, height: u16) {
        let mut needs_scroll_recalc = false;
        if self.panel_width != width {
            self.panel_width = width;
            needs_scroll_recalc = true;
        }
        if self.panel_height != height {
            self.panel_height = height;
            needs_scroll_recalc = true; // Height change also affects max_scroll
        }

        if needs_scroll_recalc {
            // If dimensions change, the current scroll_offset_vertical might be invalid.
            // It should be clamped against the new max_scroll.
            let max_s = self.max_scroll_vertical();
            self.scroll_offset_vertical = self.scroll_offset_vertical.min(max_s);
        }
    }
    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset_vertical = self.scroll_offset_vertical.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: u16) {
        let max_scroll = self.max_scroll_vertical();
        self.scroll_offset_vertical =
            self.scroll_offset_vertical.saturating_add(amount).min(max_scroll);
    }
}
