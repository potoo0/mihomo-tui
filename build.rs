use anyhow::Result;
use vergen_gix::{BuildBuilder, Emitter, GixBuilder};

fn main() -> Result<()> {
    let build = BuildBuilder::all_build()?;
    let gix = GixBuilder::all_git()?;
    Emitter::default().add_instructions(&build)?.add_instructions(&gix)?.emit()
}
