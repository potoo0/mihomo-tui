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
    /// U+2191 UPWARDS ARROW
    pub const UP: &str = "↑";

    /// U+2193 DOWNWARDS ARROW
    pub const DOWN: &str = "↓";

    /// U+2190 LEFTWARDS ARROW
    pub const LEFT: &str = "←";

    /// U+2192 RIGHTWARDS ARROW
    pub const RIGHT: &str = "→";
}

#[allow(unused)]
pub mod triangle {
    /// U+25B4 BLACK UP-POINTING SMALL TRIANGLE
    pub const UP_SMALL: &str = "▴";

    /// U+25BE BLACK DOWN-POINTING SMALL TRIANGLE
    pub const DOWN_SMALL: &str = "▾";

    /// U+25B2 BLACK UP-POINTING TRIANGLE
    pub const UP: &str = "▲";

    /// U+25BC BLACK DOWN-POINTING TRIANGLE
    pub const DOWN: &str = "▼";
}

#[allow(unused)]
pub mod dot {
    /// U+1F7E2 GREEN CIRCLE
    pub const GREEN_LARGE: &str = "🟢";

    /// U+1F534 RED CIRCLE
    pub const RED_LARGE: &str = "🔴";

    /// U+25CF BLACK CIRCLE
    pub const BLACK: &str = "●";

    /// U+25CB WHITE CIRCLE
    pub const WHITE: &str = "○";
}
