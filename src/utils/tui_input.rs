use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_input::InputRequest;

pub fn input_request(key: KeyEvent) -> Option<InputRequest> {
    use KeyCode::*;
    use tui_input::InputRequest::*;

    match (key.code, key.modifiers) {
        (Backspace, KeyModifiers::NONE) => Some(DeletePrevChar),
        (Delete, KeyModifiers::NONE) => Some(DeleteNextChar),
        (Left, KeyModifiers::NONE) => Some(GoToPrevChar),
        (Left, KeyModifiers::CONTROL) => Some(GoToPrevWord),
        (Right, KeyModifiers::NONE) => Some(GoToNextChar),
        (Right, KeyModifiers::CONTROL) => Some(GoToNextWord),
        (Char('w'), KeyModifiers::CONTROL)
        | (Backspace, KeyModifiers::META)
        | (Backspace, KeyModifiers::ALT) => Some(DeletePrevWord),
        (Delete, KeyModifiers::CONTROL) => Some(DeleteNextWord),
        (Char('y'), KeyModifiers::CONTROL) => Some(Yank),
        (Home, KeyModifiers::NONE) => Some(GoToStart),
        (End, KeyModifiers::NONE) => Some(GoToEnd),
        (Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => Some(InsertChar(c)),
        (_, _) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_editing_keys() {
        use tui_input::InputRequest::*;

        assert_eq!(
            input_request(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            Some(DeletePrevChar)
        );
        assert_eq!(
            input_request(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            Some(DeletePrevWord)
        );
        assert_eq!(
            input_request(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)),
            Some(Yank)
        );
        assert_eq!(
            input_request(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE)),
            Some(GoToStart)
        );
    }

    #[test]
    fn maps_plain_and_shift_chars() {
        use tui_input::InputRequest::*;

        assert_eq!(
            input_request(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            Some(InsertChar('a'))
        );
        assert_eq!(
            input_request(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT)),
            Some(InsertChar('A'))
        );
    }

    #[test]
    fn ignores_unmapped_modified_chars() {
        assert_eq!(input_request(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)), None);
    }
}
