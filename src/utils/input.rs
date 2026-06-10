#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    Consumed,
    Ignored,
}

impl KeyOutcome {
    pub fn is_consumed(self) -> bool {
        self == Self::Consumed
    }
}
