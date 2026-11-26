use std::borrow::Cow;

use crate::config::ProxySortConfig;
use crate::models::sort::{ProxySortField, SortDir};
use crate::ui::symbols::arrow;

pub fn label(sort: Option<&ProxySortConfig>) -> Cow<'static, str> {
    let Some(sort) = sort else {
        return rust_i18n::t!("proxy_sort.unsorted");
    };
    rust_i18n::t!(field_key(sort.field), arrow = direction_arrow(sort.dir))
}

fn field_key(field: ProxySortField) -> &'static str {
    match field {
        ProxySortField::Latency => "proxy_sort.latency",
        ProxySortField::Name => "proxy_sort.name",
    }
}

fn direction_arrow(dir: SortDir) -> &'static str {
    match dir {
        SortDir::Asc => arrow::UP,
        SortDir::Desc => arrow::DOWN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_use_localized_fields_and_ui_arrows() {
        let cases = [
            (
                ProxySortField::Latency,
                SortDir::Asc,
                "proxy_sort.latency",
                arrow::UP,
                "Latency ↑",
                "延迟↑",
            ),
            (
                ProxySortField::Latency,
                SortDir::Desc,
                "proxy_sort.latency",
                arrow::DOWN,
                "Latency ↓",
                "延迟↓",
            ),
            (ProxySortField::Name, SortDir::Asc, "proxy_sort.name", arrow::UP, "Name ↑", "名称↑"),
            (
                ProxySortField::Name,
                SortDir::Desc,
                "proxy_sort.name",
                arrow::DOWN,
                "Name ↓",
                "名称↓",
            ),
        ];

        for (field, dir, key, arrow, english, chinese) in cases {
            assert_eq!(field_key(field), key);
            assert_eq!(direction_arrow(dir), arrow);
            assert_eq!(rust_i18n::t!(key, locale = "en", arrow = arrow).as_ref(), english);
            assert_eq!(rust_i18n::t!(key, locale = "zh-CN", arrow = arrow).as_ref(), chinese);
        }

        assert_eq!(rust_i18n::t!("proxy_sort.unsorted", locale = "en").as_ref(), "Unsorted");
        assert_eq!(rust_i18n::t!("proxy_sort.unsorted", locale = "zh-CN").as_ref(), "不排序");
    }
}
