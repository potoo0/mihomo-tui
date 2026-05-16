use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SortDir {
    #[serde(alias = "ASC", alias = "Asc")]
    Asc,
    #[default]
    #[serde(alias = "DESC", alias = "Desc")]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SortSpec {
    pub col: usize,
    pub dir: SortDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProxyGroupSortField {
    #[default]
    #[serde(alias = "LATENCY", alias = "Latency")]
    Latency,
    #[serde(alias = "NAME", alias = "Name")]
    Name,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_from_yaml() {
        let yaml = "latency";
        let parsed: ProxyGroupSortField = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(parsed, ProxyGroupSortField::Latency);

        let yaml_upper = "LATENCY";
        let parsed: ProxyGroupSortField = serde_yaml_ng::from_str(yaml_upper).unwrap();
        assert_eq!(parsed, ProxyGroupSortField::Latency);

        let yaml_upper = "Latency";
        let parsed: ProxyGroupSortField = serde_yaml_ng::from_str(yaml_upper).unwrap();
        assert_eq!(parsed, ProxyGroupSortField::Latency);
    }
}
