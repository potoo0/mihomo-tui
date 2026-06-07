use std::fmt;
use std::str::FromStr;

use anyhow::{Result, anyhow, bail};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use super::LatencyThreshold;

impl fmt::Display for LatencyThreshold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{}", self.medium, self.high)
    }
}

impl FromStr for LatencyThreshold {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = value.split(',').collect();
        if parts.len() != 2 {
            bail!("Threshold must be two comma-separated numbers (e.g. 500,1000)");
        }

        let medium = parts[0]
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow!("Threshold values must be valid positive numbers"))?;
        let high = parts[1]
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow!("Threshold values must be valid positive numbers"))?;

        Ok(Self { medium, high })
    }
}

impl<'de> Deserialize<'de> for LatencyThreshold {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}
