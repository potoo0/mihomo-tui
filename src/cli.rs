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
         sidecar file next to it (default: {})",
        def.display(),
        runtime.display()
    );

    let cmd = Args::command()
        .mut_arg("config", |a| a.help(help).value_hint(ValueHint::FilePath).next_line_help(true));

    Ok(Args::from_arg_matches(&cmd.get_matches())?)
}
