use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MsgBoxSize {
    pub percent_x: u16,
    pub percent_y: u16,
}

impl MsgBoxSize {
    pub fn new(percent_x: u16, percent_y: u16) -> Self {
        Self { percent_x: percent_x.clamp(1, 100), percent_y: percent_y.clamp(1, 100) }
    }
}

impl Default for MsgBoxSize {
    fn default() -> Self {
        Self::new(80, 75)
    }
}

#[derive(Debug, Clone)]
pub struct AppMessage {
    pub title: &'static str,
    pub message: Box<str>,
    pub msg_box_size: Option<MsgBoxSize>,
}

impl AppMessage {
    pub fn msg_box_size(mut self, percent_x: u16, percent_y: u16) -> Self {
        self.msg_box_size = Some(MsgBoxSize::new(percent_x, percent_y));
        self
    }
}

impl From<(&'static str, &str)> for AppMessage {
    fn from(value: (&'static str, &str)) -> Self {
        Self { title: value.0, message: value.1.to_string().into_boxed_str(), msg_box_size: None }
    }
}

impl From<(&'static str, String)> for AppMessage {
    fn from(value: (&'static str, String)) -> Self {
        Self { title: value.0, message: value.1.into_boxed_str(), msg_box_size: None }
    }
}

impl From<(&'static str, anyhow::Error)> for AppMessage {
    fn from(value: (&'static str, anyhow::Error)) -> Self {
        Self {
            title: value.0,
            message: format!("{:?}", value.1).into_boxed_str(),
            msg_box_size: None,
        }
    }
}

impl From<(&'static str, io::Error)> for AppMessage {
    fn from(value: (&'static str, io::Error)) -> Self {
        Self {
            title: value.0,
            message: format!("{:?}", value.1).into_boxed_str(),
            msg_box_size: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msg_box_size_clamps_percentages() {
        assert_eq!(MsgBoxSize::new(0, 101), MsgBoxSize { percent_x: 1, percent_y: 100 });
    }

    #[test]
    fn test_app_message_msg_box_size_builder() {
        let message = AppMessage::from(("Title", "content")).msg_box_size(40, 25);

        assert_eq!(message.msg_box_size, Some(MsgBoxSize { percent_x: 40, percent_y: 25 }));
    }
}
