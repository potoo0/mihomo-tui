//! Parameterized presentation messages backed by the locale catalogs.
pub fn expiry(value: &str) -> String {
    rust_i18n::t!("messages.expiry", value = value).into_owned()
}
pub fn updated_at(value: &str) -> String {
    rust_i18n::t!("messages.updated_at", value = value).into_owned()
}
pub fn card_width(preferred_width: u16, actual_width: u16) -> String {
    rust_i18n::t!(
        "messages.card_width",
        preferred_width = preferred_width,
        actual_width = actual_width
    )
    .into_owned()
}
pub fn terminated(ok: usize, err: usize) -> String {
    rust_i18n::t!("messages.terminated", ok = ok, err = err).into_owned()
}
pub fn answers(count: usize) -> String {
    rust_i18n::t!("messages.answers", count = count).into_owned()
}
pub fn up_to_date(current: &str) -> String {
    rust_i18n::t!("messages.up_to_date", current = current).into_owned()
}
