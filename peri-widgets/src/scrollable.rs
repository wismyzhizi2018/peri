use ratatui::{
    layout::Rect,
    style::Style,
    text::Text,
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

/// 滚动偏移状态
///
/// 管理垂直滚动 offset，提供 ensure_visible 方法自动调整 offset 使指定行可见。
/// 逻辑从 `peri-tui/src/app/mod.rs:ensure_cursor_visible()` 迁移。
#[derive(Debug, Clone, Default)]
pub struct ScrollState {
    offset: u16,
}

impl ScrollState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_offset(offset: u16) -> Self {
        Self { offset }
    }

    pub fn offset(&self) -> u16 {
        self.offset
    }

    /// 向上滚动 delta 行
    pub fn scroll_up(&mut self, delta: u16) {
        self.offset = self.offset.saturating_sub(delta);
    }

    /// 向下滚动 delta 行（不超过 max_scroll）
    pub fn scroll_down(&mut self, delta: u16, content_height: u16, visible_height: u16) {
        let max_scroll = content_height.saturating_sub(visible_height);
        self.offset = (self.offset + delta).min(max_scroll);
    }

    /// 确保 row 行在可见视口内，自动调整 offset
    ///
    /// 从 `ensure_cursor_visible(cursor_row, scroll_offset, visible_height)` 迁移。
    pub fn ensure_visible(&mut self, row: u16, visible_height: u16) {
        if visible_height == 0 {
            self.offset = 0;
            return;
        }
        if row < self.offset {
            self.offset = row;
        } else if row >= self.offset + visible_height {
            self.offset = row.saturating_sub(visible_height.saturating_sub(1));
        }
    }

    pub fn reset(&mut self) {
        self.offset = 0;
    }
}

/// 可滚动区域——内容 + 可选滚动条
pub struct ScrollableArea<'a> {
    content: Text<'a>,
    show_scrollbar: bool,
    scrollbar_style: Style,
    max_thumb_length: Option<u16>,
}

impl<'a> ScrollableArea<'a> {
    pub fn new(content: Text<'a>) -> Self {
        Self {
            content,
            show_scrollbar: true,
            scrollbar_style: Style::default(),
            max_thumb_length: None,
        }
    }

    pub fn show_scrollbar(mut self, show: bool) -> Self {
        self.show_scrollbar = show;
        self
    }

    pub fn scrollbar_style(mut self, style: Style) -> Self {
        self.scrollbar_style = style;
        self
    }

    /// 限制滚动条滑块（thumb）的最大高度（行数）
    pub fn max_thumb_length(mut self, max: u16) -> Self {
        self.max_thumb_length = Some(max);
        self
    }

    /// 渲染可滚动区域：Paragraph + 可选 Scrollbar
    ///
    /// 自动根据内容高度和可见高度决定是否显示滚动条。
    /// 内容区域宽度减 1 留给滚动条（当 scrollbar 显示时）。
    pub fn render(self, f: &mut Frame, area: Rect, state: &mut ScrollState) {
        let content_height = self.content.height() as u16;
        let visible_height = area.height;
        let max_scroll = content_height.saturating_sub(visible_height);
        // clamp offset
        state.offset = state.offset.min(max_scroll);

        let needs_scrollbar = self.show_scrollbar && content_height > visible_height;
        let text_width = if needs_scrollbar {
            area.width.saturating_sub(1)
        } else {
            area.width
        };
        let text_area = Rect {
            width: text_width,
            ..area
        };

        let paragraph = Paragraph::new(self.content)
            .scroll((state.offset, 0))
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, text_area);

        if needs_scrollbar {
            let viewport = if let Some(max_thumb) = self.max_thumb_length {
                visible_height.min(max_thumb)
            } else {
                0
            };
            let mut scrollbar_state = ScrollbarState::new(max_scroll as usize)
                .viewport_content_length(viewport as usize)
                .position(state.offset as usize);
            let scrollbar =
                Scrollbar::new(ScrollbarOrientation::VerticalRight).style(self.scrollbar_style);
            f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
        }
    }
}

#[cfg(test)]
#[path = "scrollable_test.rs"]
mod tests;
