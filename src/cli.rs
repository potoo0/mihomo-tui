use std::path::PathBuf;

use clap::Parser;

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
    /// Path to config file
    #[arg(short, long, value_name = "CONFIG_FILE")]
    pub config: Option<PathBuf>,
}
