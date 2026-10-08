use std::path::PathBuf;

use clap::{CommandFactory, FromArgMatches, Parser, ValueHint};

use crate::config::get_config_path;
use crate::config::runtime::runtime_path_for;

#[derive(Parser, Debug)]
#[command(
    version = concat!(
        env!("CARGO_PKG_VERSION"), " - ",
        env!("VERGEN_GIT_DESCRIBE"), "(",
        env!("VERGEN_BUILD_DATE"), ")"
    ),
    about
)]
pub struct Args {
    /// Path to config file, leave empty to use default path
    #[arg(short, long, value_name = "CONFIG_FILE")]
    pub config: Option<PathBuf>,

    /// Load and save runtime UI/proxy settings (no value means true; omitted uses config)
    #[arg(long, value_name = "BOOL", action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true")]
    pub runtime_config: Option<bool>,

    /// Self-update before starting
    #[arg(long)]
    pub update: bool,
}

pub fn parse_args() -> anyhow::Result<Args> {
    // Enhance the help message for the config argument
    let def = get_config_path();
    let runtime = runtime_path_for(&def);
    let help = format!(
        "Path to config file (default: {}). Runtime UI/proxy settings are saved to the \
         sidecar file next to it (default: {}), unless runtime-config is false",
        def.display(),
        runtime.display()
    );

    let cmd = Args::command()
        .mut_arg("config", |a| a.help(help).value_hint(ValueHint::FilePath).next_line_help(true));

    Ok(Args::from_arg_matches(&cmd.get_matches())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_config_accepts_an_optional_boolean_value() {
        assert_eq!(Args::try_parse_from(["mihomo-tui"]).unwrap().runtime_config, None);
        for (value, expected) in [("true", true), ("false", false)] {
            let args = Args::try_parse_from(["mihomo-tui", "--runtime-config", value]).unwrap();
            assert_eq!(args.runtime_config, Some(expected));
            let option = format!("--runtime-config={value}");
            let args = Args::try_parse_from(["mihomo-tui", &option]).unwrap();
            assert_eq!(args.runtime_config, Some(expected));
        }
        assert_eq!(
            Args::try_parse_from(["mihomo-tui", "--runtime-config"]).unwrap().runtime_config,
            Some(true)
        );
        let args =
            Args::try_parse_from(["mihomo-tui", "--runtime-config", "--config", "custom.yaml"])
                .unwrap();
        assert_eq!(args.runtime_config, Some(true));
        assert_eq!(args.config, Some(PathBuf::from("custom.yaml")));
        for value in ["yes", "0", "invalid"] {
            assert!(Args::try_parse_from(["mihomo-tui", "--runtime-config", value]).is_err());
        }
    }
}
