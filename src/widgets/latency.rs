use ratatui::prelude::{Color, Span};

const THRESHOLD: (i64, i64) = (500, 1000);

#[derive(Debug, Clone, Copy, Default)]
pub struct Latency(Option<i64>);

#[repr(usize)]
#[derive(Debug)]
pub enum LatencyQuality {
    Fast = 0,
    Medium = 1,
    Slow = 2,
    NotConnected = 3,
}

impl Latency {
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }
}

impl From<Option<i64>> for Latency {
    fn from(value: Option<i64>) -> Self {
        Latency(value)
    }
}

impl<'a> From<Latency> for Span<'a> {
    fn from(value: Latency) -> Self {
        Span::styled(
            value.0.filter(|v| *v > 0).map(|v| format!("{}", v)).unwrap_or("-".into()),
            LatencyQuality::from(value).color(),
        )
    }
}

impl From<Latency> for LatencyQuality {
    fn from(value: Latency) -> Self {
        match value.0 {
            None => LatencyQuality::NotConnected,
            Some(d) if d <= 0 => LatencyQuality::NotConnected,
            Some(d) if d < THRESHOLD.0 => LatencyQuality::Fast,
            Some(d) if d < THRESHOLD.1 => LatencyQuality::Medium,
            Some(_) => LatencyQuality::Slow,
        }
    }
}

impl LatencyQuality {
    pub const COUNT: usize = 4;

    pub fn color(&self) -> Color {
        match self {
            LatencyQuality::Fast => Color::Rgb(0, 166, 62),
            LatencyQuality::Medium => Color::Rgb(240, 177, 0),
            LatencyQuality::Slow => Color::Rgb(251, 44, 54),
            LatencyQuality::NotConnected => Color::DarkGray,
        }
    }
}

impl From<LatencyQuality> for usize {
    fn from(value: LatencyQuality) -> Self {
        value as usize
    }
}

impl TryFrom<usize> for LatencyQuality {
    type Error = ();

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(LatencyQuality::Fast),
            1 => Ok(LatencyQuality::Medium),
            2 => Ok(LatencyQuality::Slow),
            3 => Ok(LatencyQuality::NotConnected),
            _ => Err(()),
        }
    }
}
