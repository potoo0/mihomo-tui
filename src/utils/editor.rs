use std::env;
use std::process::Command;

pub fn resolve_editor() -> String {
    if let Ok(editor) = env::var("EDITOR")
        && !editor.is_empty()
    {
        return editor;
    }

    if Command::new("vim").arg("--version").output().is_ok() {
        return "vim".to_string();
    }

    "vi".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_editor() {
        let editor = resolve_editor();
        println!("Resolved editor: {}", editor);
        assert!(!editor.is_empty());
    }
}
