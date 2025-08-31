use crate::utils::UNITS;

#[derive(Debug, Clone, Copy)]
pub struct ByteSize(pub f64);

impl ByteSize {
    pub fn fmt(&self, suffix: Option<&str>) -> String {
        human_bytes(self.0, suffix)
    }
}

impl From<u64> for ByteSize {
    fn from(value: u64) -> Self {
        ByteSize(value as f64)
    }
}

pub trait ByteSizeOptExt {
    fn fmt(&self, suffix: Option<&str>) -> String;
}

impl ByteSizeOptExt for Option<ByteSize> {
    fn fmt(&self, unit: Option<&str>) -> String {
        self.map(|b| b.fmt(unit)).unwrap_or_else(|| "-".into())
    }
}

pub fn human_bytes(bytes: f64, suffix: Option<&str>) -> String {
    let mut size = bytes;
    let mut unit_index = 0;
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    let suffix = suffix.unwrap_or("");
    if unit_index == 0 {
        format!("{} {}{}", size as u64, UNITS[unit_index], suffix)
    } else {
        format!("{:.1} {}{}", size, UNITS[unit_index], suffix)
    }
}
