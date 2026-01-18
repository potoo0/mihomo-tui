use const_format::concatcp;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Style;
use ratatui::symbols::line::{TOP_LEFT, TOP_RIGHT};
use ratatui::text::{Line, Span};

pub const TOP_TITLE_LEFT: &str = concatcp!(TOP_RIGHT, " ");
pub const TOP_TITLE_RIGHT: &str = concatcp!(" ", TOP_LEFT);

pub fn dashed_title_line<'a, S: Into<Span<'a>>>(title: S, width: u16) -> Line<'a> {
    let total = width as usize;
    let title = title.into();
    let title_len = title.width();

    if total <= title_len {
        return Line::from(title);
    }

    let dash_len = total - title_len;
    let left = dash_len >> 1;
    let right = dash_len - left;

    Line::from(vec![
        Span::raw("-".repeat(left)),
        Span::raw(" "),
        title,
        Span::raw(" "),
        Span::raw("-".repeat(right)),
    ])
}

pub fn top_title_line<S: Into<Style>>(title: &'_ str, title_style: S) -> Line<'_> {
    Line::from(vec![
        Span::raw(TOP_TITLE_LEFT),
        Span::styled(title, title_style),
        Span::raw(TOP_TITLE_RIGHT),
    ])
}

pub fn popup_area(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [area] = vertical.areas(area);
    let [area] = horizontal.areas(area);
    area
}

pub fn space_between<'a>(width: u16, left: Span<'a>, right: Span<'a>) -> Line<'a> {
    let space = width.saturating_sub((left.width() + right.width()) as u16);
    Line::from(vec![left, Span::raw(" ".repeat(space as usize)), right])
}

pub fn space_between_many<'a>(width: u16, left: Vec<Span<'a>>, right: Span<'a>) -> Line<'a> {
    let left_width: usize = left.iter().map(|s| s.width()).sum();
    let space = width.saturating_sub((left_width + right.width()) as u16);

    let mut spans = left;
    spans.push(Span::raw(" ".repeat(space as usize)));
    spans.push(right);
    Line::from(spans)
}
