use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Result, anyhow, bail};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

use super::{LatencyThreshold, MihomoApiEndpoint};

const WINDOWS_NAMED_PIPE_PREFIX: &str = r"\\.\pipe\";
const UNIX_SOCKET_PREFIX: &str = "unix:";

impl fmt::Display for MihomoApiEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(url) => url.fmt(f),
            Self::UnixSocket(path) => path.display().fmt(f),
            Self::WindowsNamedPipe(pipe) => pipe.fmt(f),
        }
    }
}

impl FromStr for MihomoApiEndpoint {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            bail!("`mihomo-api` cannot be empty");
        }

        // Windows Named Pipe
        if let Some(name) = value.strip_prefix(WINDOWS_NAMED_PIPE_PREFIX) {
            if name.is_empty() {
                bail!("Windows named pipe name cannot be empty");
            }

            return Ok(Self::WindowsNamedPipe(value.to_owned()));
        }

        // Unix Socket
        if let Some(path) = value.strip_prefix(UNIX_SOCKET_PREFIX) {
            if path.is_empty() {
                bail!("Unix socket path cannot be empty");
            }

            return Ok(Self::UnixSocket(PathBuf::from(path)));
        }

        // HTTP/HTTPS
        let url = Url::parse(value).map_err(|error| anyhow!("Invalid mihomo API URL: {error}"))?;
        if !matches!(url.scheme(), "http" | "https") || !url.has_host() {
            bail!("Mihomo HTTP API URL must use http:// or https:// and include a host");
        }
        Ok(Self::Http(url))
    }
}

impl<'de> Deserialize<'de> for MihomoApiEndpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

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

impl Serialize for LatencyThreshold {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
