use anyhow::Result;
use vergen_gitcl::{Build, Emitter, Gitcl};

fn main() -> Result<()> {
    let build = Build::all_build();
    let git = Gitcl::all_git();
    Emitter::default().add_instructions(&build)?.add_instructions(&git)?.emit()
}
