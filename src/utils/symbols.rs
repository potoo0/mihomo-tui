pub const SUPERSCRIPT: [&str; 10] = ["⁰", "¹", "²", "³", "⁴", "⁵", "⁶", "⁷", "⁸", "⁹"];

pub mod key {
    use const_format::concatcp;

    use super::arrow;

    /// U+21B5 DOWNWARDS ARROW WITH CORNER LEFTWARDS
    pub const ENTER: &str = "↵";

    /// U+21E7 U+21E4 SHIFT + TAB
    pub const SHIFT_TAB: &str = "⇧⇤";

    /// U+21E5 RIGHTWARDS ARROW TO BAR
    pub const TAB: &str = "⇥";

    pub const fn left_with_ctrl() -> &'static str {
        concatcp!(arrow::LEFT, "/C-", arrow::LEFT)
    }

    pub const fn right_with_ctrl() -> &'static str {
        concatcp!(arrow::RIGHT, "/C-", arrow::RIGHT)
    }

    pub const fn left_or_right() -> &'static str {
        concatcp!(arrow::LEFT, "/", arrow::RIGHT)
    }
}

#[allow(unused)]
pub mod arrow {
    pub const UP: &str = "↑";
    pub const DOWN: &str = "↓";
    pub const LEFT: &str = "←";
    pub const RIGHT: &str = "→";
}

#[allow(unused)]
pub mod triangle {
    pub const UP_SMALL: &str = "▴";
    pub const DOWN_SMALL: &str = "▾";

    pub const UP: &str = "▲";
    pub const DOWN: &str = "▼";
}

#[allow(unused)]
pub mod dot {
    pub const GREEN_LARGE: &str = "🟢";
    pub const RED_LARGE: &str = "🔴";
}
