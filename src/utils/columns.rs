use std::borrow::Cow;
use std::cmp::Ordering;

use crate::models::sort::SortDir;

pub struct ColDef<T> {
    #[allow(dead_code)]
    pub id: &'static str,
    pub title: &'static str,
    pub filterable: bool,
    pub sortable: bool,
    /// value accessor, used in cell rendering and filtering
    pub accessor: for<'a> fn(&'a T) -> Cow<'a, str>,
    /// sort key, optional. If None, use the string from accessor for sorting
    pub sort_key: Option<fn(&T) -> SortKey>,
}

impl<T> ColDef<T> {
    /// Compare two items based on this column definition
    #[inline]
    pub fn cmp(&self, a: &T, b: &T) -> Ordering {
        if let Some(f) = self.sort_key {
            f(a).cmp(&f(b))
        } else {
            let sa = (self.accessor)(a);
            let sb = (self.accessor)(b);
            sa.as_ref().cmp(sb.as_ref()) // use as_ref to avoid allocating
        }
    }

    #[inline]
    pub fn ordering(&self, a: &T, b: &T, dir: SortDir) -> Ordering {
        let ord = self.cmp(a, b);
        match dir {
            SortDir::Asc => ord,
            SortDir::Desc => ord.reverse(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SortKey {
    U64(u64),
    Bool(bool),
}

impl SortKey {
    pub fn cmp(&self, other: &Self) -> Ordering {
        use SortKey::*;
        match (self, other) {
            (U64(a), U64(b)) => a.cmp(b),
            (Bool(a), Bool(b)) => a.cmp(b),
            (U64(_), Bool(_)) => Ordering::Greater,
            (Bool(_), U64(_)) => Ordering::Less,
        }
    }
}
