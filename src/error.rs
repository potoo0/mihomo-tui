use std::io;

#[derive(Debug, Clone)]
pub struct UserError {
    pub title: &'static str,
    pub message: Box<str>,
}

impl From<(&'static str, &str)> for UserError {
    fn from(value: (&'static str, &str)) -> Self {
        Self { title: value.0, message: value.1.to_string().into_boxed_str() }
    }
}

impl From<(&'static str, String)> for UserError {
    fn from(value: (&'static str, String)) -> Self {
        Self { title: value.0, message: value.1.into_boxed_str() }
    }
}

impl From<(&'static str, anyhow::Error)> for UserError {
    fn from(value: (&'static str, anyhow::Error)) -> Self {
        Self { title: value.0, message: format!("{:?}", value.1).into_boxed_str() }
    }
}

impl From<(&'static str, io::Error)> for UserError {
    fn from(value: (&'static str, io::Error)) -> Self {
        Self { title: value.0, message: format!("{:?}", value.1).into_boxed_str() }
    }
}
