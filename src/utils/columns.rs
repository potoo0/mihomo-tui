use std::borrow::Cow;
use std::cmp::Ordering;

use ratatui::layout::Constraint;

use crate::models::sort::SortDir;

pub trait TextResolver<T> {
    fn resolve<'row>(&self, col: &ColDef<T>, item: &'row T, text: Cow<'row, str>)
    -> Cow<'row, str>;
}

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

pub struct TableColDef<T> {
    pub col: ColDef<T>,
    pub constraint: Constraint,
}

impl<T> AsRef<ColDef<T>> for ColDef<T> {
    fn as_ref(&self) -> &ColDef<T> {
        self
    }
}

impl<T> AsRef<ColDef<T>> for TableColDef<T> {
    fn as_ref(&self) -> &ColDef<T> {
        &self.col
    }
}

impl<T> ColDef<T> {
    /// Compare two items based on this column definition
    #[allow(dead_code)]
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

    /// Compare two items using the provided text resolver when this column has
    /// no typed sort key.
    #[inline]
    pub fn cmp_with_text_resolver(&self, a: &T, b: &T, resolver: &dyn TextResolver<T>) -> Ordering {
        if let Some(f) = self.sort_key {
            f(a).cmp(&f(b))
        } else {
            let sa = resolver.resolve(self, a, (self.accessor)(a));
            let sb = resolver.resolve(self, b, (self.accessor)(b));
            sa.as_ref().cmp(sb.as_ref())
        }
    }

    #[allow(dead_code)]
    #[inline]
    pub fn ordering(&self, a: &T, b: &T, dir: SortDir) -> Ordering {
        let ord = self.cmp(a, b);
        match dir {
            SortDir::Asc => ord,
            SortDir::Desc => ord.reverse(),
        }
    }

    #[inline]
    pub fn ordering_with_text_resolver(
        &self,
        a: &T,
        b: &T,
        dir: SortDir,
        resolver: &dyn TextResolver<T>,
    ) -> Ordering {
        let ord = self.cmp_with_text_resolver(a, b, resolver);
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
