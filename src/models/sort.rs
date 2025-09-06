#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SortDir {
    Asc,
    #[default]
    Desc,
}

impl SortDir {
    #[inline]
    pub fn toggle(self) -> Self {
        match self {
            SortDir::Asc => SortDir::Desc,
            SortDir::Desc => SortDir::Asc,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SortSpec {
    pub col: usize,
    pub dir: SortDir,
}
