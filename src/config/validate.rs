use std::num::NonZeroUsize;

use anyhow::{Result, anyhow, bail};
use url::Url;

use crate::config::{Config, LatencyThreshold, ProxySetting};

impl Config {
    pub fn validate(&self) -> Result<()> {
        self.proxy_setting.validate()?;
        Ok(())
    }
}

impl ProxySetting {
    pub fn validate(&self) -> Result<()> {
        Self::validate_test_url(&self.test_url)?;
        Self::validate_test_timeout(self.test_timeout)?;
        Self::validate_latency_threshold(self.latency_threshold)?;
        Ok(())
    }

    pub fn validate_test_url(value: &str) -> Result<()> {
        if value.is_empty() {
            bail!("URL cannot be empty");
        }
        if !value.starts_with("http://") && !value.starts_with("https://") {
            bail!("URL must start with http:// or https://");
        }

        Url::parse(value).map_err(|e| anyhow!("Invalid URL: {}", e))?;
        Ok(())
    }

    pub fn validate_test_timeout(value: NonZeroUsize) -> Result<()> {
        if value.get() <= 60000 {
            Ok(())
        } else {
            bail!("Timeout must be between 1 and 60000 milliseconds");
        }
    }

    pub fn validate_latency_threshold(value: LatencyThreshold) -> Result<()> {
        if value.medium == 0 || value.high == 0 {
            bail!("Threshold values must be valid positive numbers");
        }
        if value.medium >= value.high {
            bail!("Threshold must satisfy medium < high");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_threshold_parse() {
        assert_eq!(
            "500,1000".parse::<LatencyThreshold>().unwrap(),
            LatencyThreshold { medium: 500, high: 1000 }
        );
    }

    #[test]
    fn test_latency_threshold_invalid_order() {
        let err =
            ProxySetting::validate_latency_threshold(LatencyThreshold { medium: 1000, high: 500 })
                .unwrap_err();
        assert!(err.to_string().contains("Threshold must satisfy medium < high"));
    }

    #[test]
    fn test_proxy_test_timeout_range() {
        assert!(ProxySetting::validate_test_timeout(NonZeroUsize::new(1).unwrap()).is_ok());
        assert!(ProxySetting::validate_test_timeout(NonZeroUsize::new(60000).unwrap()).is_ok());
        assert!(ProxySetting::validate_test_timeout(NonZeroUsize::new(60001).unwrap()).is_err());
    }
}
