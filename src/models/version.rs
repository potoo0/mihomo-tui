use std::fmt::Display;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Version {
    pub meta: bool,
    pub version: String,
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.meta {
            write!(f, "Clash(Meta) {}", self.version)
        } else {
            write!(f, "Clash {}", self.version)
        }
    }
}
