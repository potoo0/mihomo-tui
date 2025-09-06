#[derive(Debug, Clone, PartialEq, Default)]
pub struct SearchQuery {
    pub query: Option<String>,
    pub order_by: Option<OrderBy>,
}

/// OrderBy is a tuple of (index, desc)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderBy(pub usize, pub bool);
// pub enum OrderBy {
//     Asc(usize),
//     Desc(usize),
// }
//
// impl From<(usize, bool)> for OrderBy {
//     fn from((index, desc): (usize, bool)) -> Self {
//         if desc {
//             OrderBy::Desc(index)
//         } else {
//             OrderBy::Asc(index)
//         }
//     }
// }
