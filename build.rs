use anyhow::Result;
use vergen_gitcl::{BuildBuilder, Emitter, GitclBuilder};

fn main() -> Result<()> {
    let build = BuildBuilder::all_build()?;
    let git = GitclBuilder::all_git()?;
    Emitter::default().add_instructions(&build)?.add_instructions(&git)?.emit()
}
