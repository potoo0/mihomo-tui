use serde::Deserialize;

/// Shortcuts are keyboards shortcuts available to the user to interact with the UI.
#[derive(Debug)]
pub struct Shortcut {
    pub key: &'static str,
    pub description: &'static str,
}

impl Shortcut {
    pub fn new(key: &'static str, description: &'static str) -> Self {
        Self { key, description }
    }
}
