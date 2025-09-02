use std::sync::Arc;

use color_eyre::Result;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, BorderType, Cell, Chart, Dataset, GraphType, Padding, Row, Table,
};

use crate::components::{AppState, Component, ComponentId};
use crate::palette;
use crate::utils::byte_size::{ByteSizeOptExt, human_bytes};
use crate::utils::{axis_bounds, axis_labels};

type Series = Vec<(f64, f64)>;

#[derive(Debug, Default)]
pub struct OverviewComponent {}

impl OverviewComponent {
    fn render_header(&mut self, frame: &mut Frame, area: Rect, state: &AppState) {
        let conn_stat = Arc::clone(&state.conn_stat).lock().unwrap().clone();
        let conn_stat = conn_stat.as_ref();
        let traffic = {
            let guard = state.traffic.lock().unwrap();
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
                Span::styled("↑ ", Style::default().fg(palette::UP)),
                Span::raw(
                    traffic
                        .map(|(v, _)| human_bytes(v as f64, Some("/s")))
                        .unwrap_or("-".into()),
                )
                .bold(),
                Span::raw(" / ").dark_gray(),
                Span::raw(
                    traffic
                        .map(|(_, v)| human_bytes(v as f64, Some("/s")))
                        .unwrap_or("-".into()),
                )
                .bold(),
                Span::styled(" ↓", Style::default().fg(palette::DOWN)),
            ]),
            Line::from(vec![
                Span::styled("↑ ", Style::default().fg(palette::UP)),
                Span::raw(conn_stat.map(|s| s.up_total).fmt(None)).bold(),
                Span::raw(" / ").dark_gray(),
                Span::raw(conn_stat.map(|s| s.down_total).fmt(None)).bold(),
                Span::styled(" ↓", Style::default().fg(palette::DOWN)),
            ]),
            Line::from(
                conn_stat
                    .map(|s| format!("{}", s.conns_size))
                    .unwrap_or("-".into()),
            )
            .centered(),
            Line::from(conn_stat.map(|s| s.memory).fmt(None)).centered(),
        ];
        let table = Table::new(
            vec![Row::new(
                cells_content.into_iter().map(|c| Cell::from(c.centered())),
            )],
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

    fn render_charts(&mut self, frame: &mut Frame, area: Rect, state: &AppState) {
        let outer = Block::bordered()
            .border_type(BorderType::Rounded)
            .padding(Padding::new(1, 1, 1, 1));
        frame.render_widget(outer.clone(), area);

        let chunks = Layout::horizontal([
            Constraint::Percentage(49),
            Constraint::Percentage(1),
            Constraint::Fill(1),
        ])
        .split(outer.inner(area));

        let traffic = Self::split_traffic(state);
        self.render_traffic_chart(frame, chunks[0], traffic);
        let memory: Series = state
            .memory
            .lock()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, m)| (i as f64, m.used as f64))
            .collect();
        self.render_memory_chart(frame, chunks[2], memory);
    }

    fn split_traffic(state: &AppState) -> [Series; 2] {
        let traffic = state.traffic.lock().unwrap();
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
                (
                    0f64,
                    traffic[index].iter().map(|(_, y)| *y).fold(1.0, f64::max),
                )
            } else {
                (
                    traffic[index].iter().map(|(_, y)| *y).fold(-1.0, f64::min),
                    0f64,
                )
            };
            let labels: Vec<String> = axis_labels(bound.0, bound.1)
                .into_iter()
                .map(|s| {
                    if s.len() < 10 {
                        format!("{:>10}", s)
                    } else {
                        s
                    }
                })
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
        let dataset = Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .data(&data);

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

impl Component for OverviewComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Overview
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, state: &AppState) -> Result<()> {
        let chunks = Layout::vertical([Constraint::Length(4), Constraint::Min(0)]).split(area);

        self.render_header(frame, chunks[0], state);
        self.render_charts(frame, chunks[1], state);
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
