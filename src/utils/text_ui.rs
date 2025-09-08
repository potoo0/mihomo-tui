use const_format::concatcp;
use ratatui::style::Style;
use ratatui::symbols::line::{TOP_LEFT, TOP_RIGHT};
use ratatui::text::{Line, Span};

pub const TOP_TITLE_LEFT: &str = concatcp!(TOP_RIGHT, " ");
pub const TOP_TITLE_RIGHT: &str = concatcp!(" ", TOP_LEFT);

pub fn top_title_line<S: Into<Style>>(title: &'_ str, title_style: S) -> Line<'_> {
    Line::from(vec![
        Span::raw(TOP_TITLE_LEFT),
        Span::styled(title, title_style),
        Span::raw(TOP_TITLE_RIGHT),
    ])
}
