use crate::config::ProxySortConfig;
use crate::models::sort::{ProxySortField, SortDir};

pub(crate) fn next_sort_field(sort: Option<ProxySortConfig>) -> Option<ProxySortConfig> {
    match sort {
        None => Some(ProxySortConfig { field: ProxySortField::Latency, dir: SortDir::Asc }),
        Some(old) => match old.field {
            ProxySortField::Latency => {
                Some(ProxySortConfig { field: ProxySortField::Name, dir: old.dir })
            }
            ProxySortField::Name => None,
        },
    }
}

pub(crate) fn toggle_sort_direction(sort: Option<ProxySortConfig>) -> Option<ProxySortConfig> {
    sort.map(|old| ProxySortConfig { dir: old.dir.toggle(), ..old })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_sort_field_cycles_latency_name_none() {
        let first = next_sort_field(None).unwrap();
        assert_eq!(first.field, ProxySortField::Latency);
        assert_eq!(first.dir, SortDir::Asc);

        let second = next_sort_field(Some(first)).unwrap();
        assert_eq!(second.field, ProxySortField::Name);
        assert_eq!(second.dir, SortDir::Asc);

        assert!(next_sort_field(Some(second)).is_none());
    }

    #[test]
    fn toggle_sort_direction_preserves_none_and_field() {
        assert!(toggle_sort_direction(None).is_none());

        let toggled = toggle_sort_direction(Some(ProxySortConfig {
            field: ProxySortField::Name,
            dir: SortDir::Asc,
        }))
        .unwrap();

        assert_eq!(toggled.field, ProxySortField::Name);
        assert_eq!(toggled.dir, SortDir::Desc);
    }
}
