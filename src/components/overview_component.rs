use std::sync::{Arc, Mutex};

use anyhow::Result;
use circular_buffer::CircularBuffer;
use const_format::concatcp;
use futures_util::{StreamExt, TryStreamExt, future};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Cell, Chart, Dataset, GraphType, Padding, Row, Table,
};
use tokio::sync::watch::Receiver;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::action::Action;
use crate::api::Api;
use crate::components::{BUFFER_SIZE, Component, ComponentId};
use crate::models::{ConnectionStats, Memory, Traffic};
use crate::palette;
use crate::utils::axis::{axis_bounds, axis_labels};
use crate::utils::byte_size::{ByteSizeOptExt, human_bytes};
use crate::utils::symbols::arrow;

const UP: &str = concatcp!(arrow::UP, " ");
const DOWN: &str = concatcp!(" ", arrow::DOWN);

type Series = Vec<(f64, f64)>;

#[derive(Debug)]
pub struct OverviewComponent {
    api: Option<Arc<Api>>,
    token: CancellationToken,

    stats_rx: Receiver<Option<ConnectionStats>>,
    memory: Arc<Mutex<CircularBuffer<BUFFER_SIZE, Memory>>>,
    traffic: Arc<Mutex<CircularBuffer<BUFFER_SIZE, Traffic>>>,
}

impl OverviewComponent {
    pub fn new(stats_rx: Receiver<Option<ConnectionStats>>) -> Self {
        Self {
            api: Default::default(),
            token: Default::default(),

            stats_rx,
            memory: Default::default(),
            traffic: Default::default(),
        }
    }

    fn load_memory(&mut self) -> Result<()> {
        info!("Loading memory");
        let token = self.token.clone();
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.memory);

        tokio::task::Builder::new().name("memory-loader").spawn(async move {
            let stream = match api.get_memory().await {
                Ok(stream) => stream,
                Err(e) => {
                    error!(error = ?e, "Failed to get memory stream");
                    return;
                }
            };
            stream
                .take_until(token.cancelled())
                .inspect_err(|e| warn!("Failed to parse memory: {e}"))
                .filter_map(|res| future::ready(res.ok()))
                .for_each(|record| {
                    if record.used > 0 {
                        store.lock().unwrap().push_back(record);
                    }
                    future::ready(())
                })
                .await;
        })?;
        Ok(())
    }

    fn load_traffic(&mut self) -> Result<()> {
        info!("Loading traffic");
        let token = self.token.clone();
        let api = Arc::clone(self.api.as_ref().unwrap());
        let store = Arc::clone(&self.traffic);

        tokio::task::Builder::new().name("traffic-loader").spawn(async move {
            let stream = match api.get_traffic().await {
                Ok(stream) => stream,
                Err(e) => {
                    error!(error = ?e, "Failed to get traffic stream");
                    return;
                }
            };
            stream
                .take_until(token.cancelled())
                .inspect_err(|e| warn!("Failed to parse traffic: {e}"))
                .filter_map(|res| future::ready(res.ok()))
                .for_each(|record| {
                    store.lock().unwrap().push_back(record);
                    future::ready(())
                })
                .await;
        })?;
        Ok(())
    }

    fn render_header(&mut self, frame: &mut Frame, area: Rect) {
        let conn_stats = {
            let stats = self.stats_rx.borrow();
            let stats = stats.as_ref();
            (
                stats.map(|s| s.up_total).fmt(None),
                stats.map(|s| s.down_total).fmt(None),
                stats.map(|s| s.conns_size.to_string()).unwrap_or("-".into()),
                stats.map(|s| s.memory).fmt(None),
            )
        };
        let traffic = {
            let guard = self.traffic.lock().unwrap();
            guard.back().map(|t| (t.up, t.down))
        };

        let header = Row::new([
            Cell::from(Line::from("Rate").centered()),
            Cell::from(Line::from("Total").centered()),
            Cell::from(Line::from("Conns").centered()),
            Cell::from(Line::from("Memory").centered()),
        ]);

        let cells_content = vec![
            Line::from(vec![
                Span::styled(UP, Style::default().fg(palette::UP)),
                Span::raw(
                    traffic.map(|(v, _)| human_bytes(v as f64, Some("/s"))).unwrap_or("-".into()),
                )
                .bold(),
                Span::raw(" / ").dark_gray(),
                Span::raw(
                    traffic.map(|(_, v)| human_bytes(v as f64, Some("/s"))).unwrap_or("-".into()),
                )
                .bold(),
                Span::styled(DOWN, Style::default().fg(palette::DOWN)),
            ]),
            Line::from(vec![
                Span::styled(UP, Style::default().fg(palette::UP)),
                Span::raw(conn_stats.0).bold(),
                Span::raw(" / ").dark_gray(),
                Span::raw(conn_stats.1).bold(),
                Span::styled(DOWN, Style::default().fg(palette::DOWN)),
            ]),
            Line::from(conn_stats.2).centered(),
            Line::from(conn_stats.3).centered(),
        ];

        let table = Table::new(
            vec![Row::new(cells_content.into_iter().map(|c| Cell::from(c.centered())))],
            [
                Constraint::Ratio(2, 5),
                Constraint::Ratio(2, 5),
                Constraint::Ratio(1, 5),
                Constraint::Ratio(1, 5),
            ],
        )
        .header(header)
        .column_spacing(2)
        .block(Block::bordered().border_type(BorderType::Rounded));
        frame.render_widget(table, area);
    }

    fn render_charts(&mut self, frame: &mut Frame, area: Rect) {
        let outer =
            Block::bordered().border_type(BorderType::Rounded).padding(Padding::new(1, 1, 1, 1));
        frame.render_widget(outer.clone(), area);

        let chunks = Layout::horizontal([
            Constraint::Percentage(49),
            Constraint::Percentage(1),
            Constraint::Fill(1),
        ])
        .split(outer.inner(area));

        let traffic = self.split_traffic();
        self.render_traffic_chart(frame, chunks[0], traffic);
        let memory: Series = self
            .memory
            .lock()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, m)| (i as f64, m.used as f64))
            .collect();
        self.render_memory_chart(frame, chunks[2], memory);
    }

    fn split_traffic(&mut self) -> [Series; 2] {
        let traffic = self.traffic.lock().unwrap();
        let mut up_points = Vec::with_capacity(traffic.len());
        let mut down_points = Vec::with_capacity(traffic.len());

        for (i, t) in traffic.iter().enumerate() {
            up_points.push((i as f64, t.up as f64));
            down_points.push((i as f64, -(t.down as f64)));
        }

        [up_points, down_points]
    }

    fn render_traffic_chart(&mut self, frame: &mut Frame, area: Rect, traffic: [Series; 2]) {
        let colors = [palette::UP, palette::DOWN];
        let chunks =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
        let blocks = [
            Some(Block::default().title(Line::from("Traffic chart").cyan().bold().centered())),
            None,
        ];
        for index in 0..2 {
            let bound = if index == 0 {
                (0f64, traffic[index].iter().map(|(_, y)| *y).fold(1.0, f64::max))
            } else {
                (traffic[index].iter().map(|(_, y)| *y).fold(-1.0, f64::min), 0f64)
            };
            let labels: Vec<String> = axis_labels(bound.0, bound.1)
                .into_iter()
                .map(|s| if s.len() < 10 { format!("{:>10}", s) } else { s })
                .collect();
            let dataset = Dataset::default()
                .marker(Marker::Braille)
                .style(colors[index])
                .graph_type(GraphType::Line)
                .data(&traffic[index]);

            let mut chart = Chart::new(vec![dataset])
                .x_axis(Axis::default().bounds([0.0, traffic[index].len() as f64]))
                .y_axis(
                    Axis::default()
                        .style(Style::default().dark_gray())
                        .bounds([bound.0, bound.1])
                        .labels(labels),
                );
            if let Some(b) = &blocks[index] {
                chart = chart.block(b.clone());
            }
            frame.render_widget(chart, chunks[index]);
        }
    }

    fn render_memory_chart(&mut self, frame: &mut Frame, area: Rect, data: Vec<(f64, f64)>) {
        let dataset =
            Dataset::default().marker(Marker::Braille).graph_type(GraphType::Line).data(&data);

        let bounds = axis_bounds(&data);
        let chart = Chart::new(vec![dataset])
            .block(
                Block::default()
                    .padding(Padding::left(1))
                    .title(Line::from("Memory chart").cyan().bold().centered()),
            )
            .x_axis(Axis::default().bounds([0.0, data.len() as f64]))
            .y_axis(
                Axis::default()
                    .style(Style::default().dark_gray())
                    .bounds([bounds.0, bounds.1])
                    .labels(axis_labels(bounds.0, bounds.1)),
            );
        frame.render_widget(chart, area);
    }
}

impl Drop for OverviewComponent {
    fn drop(&mut self) {
        self.token.cancel();
        info!("`OverviewComponent` dropped, background task cancelled");
    }
}

impl Component for OverviewComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Overview
    }

    fn init(&mut self, api: Arc<Api>) -> Result<()> {
        self.api = Some(api);
        self.token = CancellationToken::new();
        self.load_memory()?;
        self.load_traffic()?;
        Ok(())
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        if matches!(action, Action::Quit) {
            self.token.cancel();
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let chunks = Layout::vertical([Constraint::Length(4), Constraint::Min(0)]).split(area);

        self.render_header(frame, chunks[0]);
        self.render_charts(frame, chunks[1]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;
    use ratatui::widgets::{Block, BorderType};

    #[test]
    fn test_border() {
        let b = Block::bordered().border_type(BorderType::Rounded);
        let area = Rect::new(0, 0, 10, 5);
        let inner = b.inner(area);
        assert_eq!(inner, Rect::new(1, 1, 8, 3));
    }
}
