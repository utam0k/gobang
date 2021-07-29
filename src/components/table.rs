use super::{
    utils::scroll_vertical::VerticalScroll, Component, DrawableComponent, EventState,
    TableValueComponent,
};
use crate::components::command::CommandInfo;
use crate::event::Key;
use anyhow::Result;
use std::convert::From;
use tui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
    Frame,
};
use unicode_width::UnicodeWidthStr;

pub struct TableComponent {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub eod: bool,
    pub selected_row: TableState,
    selected_column: usize,
    selection_area_corner: Option<(usize, usize)>,
    column_page_start: std::cell::Cell<usize>,
    scroll: VerticalScroll,
}

impl Default for TableComponent {
    fn default() -> Self {
        Self {
            selected_row: TableState::default(),
            headers: vec![],
            rows: vec![],
            selected_column: 0,
            selection_area_corner: None,
            column_page_start: std::cell::Cell::new(0),
            scroll: VerticalScroll::new(),
            eod: false,
        }
    }
}

impl TableComponent {
    pub fn new(rows: Vec<Vec<String>>, headers: Vec<String>) -> Self {
        let mut selected_row = TableState::default();
        if !rows.is_empty() {
            selected_row.select(None);
            selected_row.select(Some(0))
        }
        Self {
            headers,
            rows,
            selected_row,
            ..Self::default()
        }
    }

    fn reset(&mut self) {
        self.selection_area_corner = None;
    }

    pub fn end(&mut self) {
        self.eod = true;
    }

    fn next_row(&mut self, lines: usize) {
        let i = match self.selected_row.selected() {
            Some(i) => {
                if i + lines >= self.rows.len() {
                    Some(self.rows.len() - 1)
                } else {
                    Some(i + lines)
                }
            }
            None => None,
        };
        self.reset();
        self.selected_row.select(i);
    }

    fn previous_row(&mut self, lines: usize) {
        let i = match self.selected_row.selected() {
            Some(i) => {
                if i <= lines {
                    Some(0)
                } else {
                    Some(i - lines)
                }
            }
            None => None,
        };
        self.reset();
        self.selected_row.select(i);
    }

    fn scroll_top(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.reset();
        self.selected_row.select(Some(0));
    }

    fn scroll_bottom(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.reset();
        self.selected_row.select(Some(self.rows.len() - 1));
    }

    fn next_column(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        if self.selected_column >= self.headers.len().saturating_sub(1) {
            return;
        }
        self.reset();
        self.selected_column += 1;
    }

    fn previous_column(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        if self.selected_column == 0 {
            return;
        }
        self.reset();
        self.selected_column -= 1;
    }

    fn expand_selected_area_x(&mut self, positive: bool) {
        if self.selection_area_corner.is_none() {
            self.selection_area_corner = Some((
                self.selected_column,
                self.selected_row.selected().unwrap_or(0),
            ));
        }
        if let Some((x, y)) = self.selection_area_corner {
            self.selection_area_corner = Some((
                if positive {
                    (x + 1).min(self.headers.len().saturating_sub(1))
                } else {
                    x.saturating_sub(1)
                },
                y,
            ));
        }
    }

    fn expand_selected_area_y(&mut self, positive: bool) {
        if self.selection_area_corner.is_none() {
            self.selection_area_corner = Some((
                self.selected_column,
                self.selected_row.selected().unwrap_or(0),
            ));
        }
        if let Some((x, y)) = self.selection_area_corner {
            self.selection_area_corner = Some((
                x,
                if positive {
                    (y + 1).min(self.rows.len().saturating_sub(1))
                } else {
                    y.saturating_sub(1)
                },
            ));
        }
    }

    pub fn selected_cells(&self) -> Option<String> {
        if let Some((x, y)) = self.selection_area_corner {
            let selected_row_index = self.selected_row.selected()?;
            return Some(
                self.rows[y.min(selected_row_index)..y.max(selected_row_index) + 1]
                    .iter()
                    .map(|row| {
                        row[x.min(self.selected_column)..x.max(self.selected_column) + 1].join(",")
                    })
                    .collect::<Vec<String>>()
                    .join("\n"),
            );
        }
        self.rows
            .get(self.selected_row.selected()?)?
            .get(self.selected_column)
            .map(|cell| cell.to_string())
    }

    fn selected_column_index(&self) -> usize {
        if let Some((x, _)) = self.selection_area_corner {
            return x;
        }
        self.selected_column
    }

    fn is_selected_cell(
        &self,
        row_index: usize,
        column_index: usize,
        selected_column_index: usize,
    ) -> bool {
        if let Some((x, y)) = self.selection_area_corner {
            let x_in_page = x
                .saturating_add(1)
                .saturating_sub(self.column_page_start.get());
            return matches!(
                self.selected_row.selected(),
                Some(selected_row_index)
                if (x_in_page.min(selected_column_index).max(1)..x_in_page.max(selected_column_index) + 1)
                    .contains(&column_index)
                    && (y.min(selected_row_index)..y.max(selected_row_index) + 1)
                        .contains(&row_index)
            );
        }
        matches!(
            self.selected_row.selected(),
            Some(selected_row_index) if row_index == selected_row_index &&  column_index == selected_column_index
        )
    }

    fn is_number_column(&self, row_index: usize, column_index: usize) -> bool {
        matches!(
            self.selected_row.selected(),
            Some(selected_row_index) if row_index == selected_row_index && 0 == column_index
        )
    }

    fn headers(&self, left: usize, right: usize) -> Vec<String> {
        let mut headers = self.headers.clone()[left..right].to_vec();
        headers.insert(0, "".to_string());
        headers
    }

    fn rows(&self, left: usize, right: usize) -> Vec<Vec<String>> {
        let rows = self
            .rows
            .iter()
            .map(|row| row.to_vec())
            .collect::<Vec<Vec<String>>>();
        let mut new_rows: Vec<Vec<String>> =
            rows.iter().map(|row| row[left..right].to_vec()).collect();
        for (index, row) in new_rows.iter_mut().enumerate() {
            row.insert(0, (index + 1).to_string())
        }
        new_rows
    }

    fn calculate_cell_widths(
        &self,
        area_width: u16,
    ) -> (usize, Vec<String>, Vec<Vec<String>>, Vec<Constraint>) {
        if self.rows.is_empty() {
            return (0, Vec::new(), Vec::new(), Vec::new());
        }
        if self.selected_column_index() < self.column_page_start.get() {
            self.column_page_start.set(self.selected_column_index());
        }

        let far_right_column_index = self.selected_column_index();
        let mut column_index = self.selected_column_index();
        let number_column_width = (self.rows.len() + 1).to_string().width() as u16;
        let mut widths = Vec::new();
        loop {
            let length = self
                .rows
                .iter()
                .map(|row| {
                    row.get(column_index)
                        .map_or(String::new(), |cell| cell.to_string())
                        .width()
                })
                .collect::<Vec<usize>>()
                .iter()
                .max()
                .map_or(3, |v| {
                    *v.max(
                        &self
                            .headers
                            .get(column_index)
                            .map_or(3, |header| header.to_string().width()),
                    )
                    .clamp(&3, &20)
                });
            if widths.iter().map(|(_, width)| width).sum::<usize>() + length + widths.len()
                >= area_width.saturating_sub(number_column_width) as usize
            {
                column_index += 1;
                break;
            }
            widths.push((self.headers[column_index].clone(), length));
            if column_index == self.column_page_start.get() {
                break;
            }
            column_index -= 1;
        }
        widths.reverse();

        let far_left_column_index = column_index;
        let selected_column_index = widths.len().saturating_sub(1);
        let mut column_index = far_right_column_index + 1;
        while widths.iter().map(|(_, width)| width).sum::<usize>() + widths.len()
            <= area_width.saturating_sub(number_column_width) as usize
        {
            let length = self
                .rows
                .iter()
                .map(|row| {
                    row.get(column_index)
                        .map_or(String::new(), |cell| cell.to_string())
                        .width()
                })
                .collect::<Vec<usize>>()
                .iter()
                .max()
                .map_or(3, |v| {
                    *v.max(
                        self.headers
                            .iter()
                            .map(|header| header.to_string().width())
                            .collect::<Vec<usize>>()
                            .get(column_index)
                            .unwrap_or(&3),
                    )
                    .clamp(&3, &20)
                });
            match self.headers.get(column_index) {
                Some(header) => {
                    widths.push((header.to_string(), length));
                }
                None => break,
            }
            column_index += 1
        }
        if self.selected_column_index() != self.headers.len().saturating_sub(1) {
            widths.pop();
        }
        let far_right_column_index = column_index;
        let mut constraints = widths
            .iter()
            .map(|(_, width)| Constraint::Length(*width as u16))
            .collect::<Vec<Constraint>>();
        if self.selected_column_index() != self.headers.len().saturating_sub(1) {
            constraints.push(Constraint::Min(10));
        }
        constraints.insert(0, Constraint::Length(number_column_width));
        self.column_page_start.set(far_left_column_index);

        (
            self.selection_area_corner
                .map_or(selected_column_index + 1, |(x, _)| {
                    if x > self.selected_column {
                        (selected_column_index + 1)
                            .saturating_sub(x.saturating_sub(self.selected_column))
                    } else {
                        (selected_column_index + 1)
                            .saturating_add(self.selected_column.saturating_sub(x))
                    }
                }),
            self.headers(far_left_column_index, far_right_column_index),
            self.rows(far_left_column_index, far_right_column_index),
            constraints,
        )
    }
}

impl DrawableComponent for TableComponent {
    fn draw<B: Backend>(&mut self, f: &mut Frame<B>, area: Rect, focused: bool) -> Result<()> {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(3), Constraint::Length(5)])
            .split(area);

        self.selected_row.selected().map_or_else(
            || {
                self.scroll.reset();
            },
            |selection| {
                self.scroll.update(
                    selection,
                    self.rows.len(),
                    layout[1].height.saturating_sub(2) as usize,
                );
            },
        );

        TableValueComponent::new(self.selected_cells().unwrap_or_default())
            .draw(f, layout[0], focused)?;

        let block = Block::default().borders(Borders::ALL).title("Records");
        let (selected_column_index, headers, rows, constraints) =
            self.calculate_cell_widths(block.inner(layout[1]).width);
        let header_cells = headers.iter().enumerate().map(|(column_index, h)| {
            Cell::from(h.to_string()).style(if selected_column_index == column_index {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            })
        });
        let header = Row::new(header_cells).height(1).bottom_margin(1);
        let rows = rows.iter().enumerate().map(|(row_index, item)| {
            let height = item
                .iter()
                .map(|content| content.chars().filter(|c| *c == '\n').count())
                .max()
                .unwrap_or(0)
                + 1;
            let cells = item.iter().enumerate().map(|(column_index, c)| {
                Cell::from(c.to_string()).style(
                    if self.is_selected_cell(row_index, column_index, selected_column_index) {
                        Style::default().bg(Color::Blue)
                    } else if self.is_number_column(row_index, column_index) {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                )
            });
            Row::new(cells).height(height as u16).bottom_margin(1)
        });

        let table = Table::new(rows)
            .header(header)
            .block(block)
            .style(if focused {
                Style::default()
            } else {
                Style::default().fg(Color::DarkGray)
            })
            .widths(&constraints);
        let mut state = self.selected_row.clone();
        f.render_stateful_widget(
            table,
            layout[1],
            if let Some((_, y)) = self.selection_area_corner {
                state.select(Some(y));
                &mut state
            } else {
                &mut self.selected_row
            },
        );

        self.scroll.draw(f, layout[1]);
        Ok(())
    }
}

impl Component for TableComponent {
    fn commands(&self, out: &mut Vec<CommandInfo>) {}

    fn event(&mut self, key: Key) -> Result<EventState> {
        match key {
            Key::Char('h') => {
                self.previous_column();
                return Ok(EventState::Consumed);
            }
            Key::Char('j') => {
                self.next_row(1);
                return Ok(EventState::NotConsumed);
            }
            Key::Ctrl('d') => {
                self.next_row(10);
                return Ok(EventState::NotConsumed);
            }
            Key::Char('k') => {
                self.previous_row(1);
                return Ok(EventState::Consumed);
            }
            Key::Ctrl('u') => {
                self.previous_row(10);
                return Ok(EventState::Consumed);
            }
            Key::Char('g') => {
                self.scroll_top();
                return Ok(EventState::Consumed);
            }
            Key::Char('G') => {
                self.scroll_bottom();
                return Ok(EventState::Consumed);
            }
            Key::Char('l') => {
                self.next_column();
                return Ok(EventState::Consumed);
            }
            Key::Char('H') => {
                self.expand_selected_area_x(false);
                return Ok(EventState::Consumed);
            }
            Key::Char('K') => {
                self.expand_selected_area_y(false);
                return Ok(EventState::Consumed);
            }
            Key::Char('J') => {
                self.expand_selected_area_y(true);
                return Ok(EventState::Consumed);
            }
            Key::Char('L') => {
                self.expand_selected_area_x(true);
                return Ok(EventState::Consumed);
            }
            _ => (),
        }
        Ok(EventState::NotConsumed)
    }
}

#[cfg(test)]
mod test {
    use super::TableComponent;
    use tui::layout::Constraint;

    #[test]
    fn test_headers() {
        let mut component = TableComponent::default();
        component.headers = vec!["a", "b", "c"].iter().map(|h| h.to_string()).collect();
        assert_eq!(component.headers(1, 2), vec!["", "b"])
    }

    #[test]
    fn test_rows() {
        let mut component = TableComponent::default();
        component.rows = vec![
            vec!["a", "b", "c"].iter().map(|h| h.to_string()).collect(),
            vec!["d", "e", "f"].iter().map(|h| h.to_string()).collect(),
        ];
        assert_eq!(component.rows(1, 2), vec![vec!["1", "b"], vec!["2", "e"]],)
    }

    #[test]
    fn test_expand_selected_area_x_left() {
        // before
        //    1  2  3
        // 1  a  b  c
        // 2  d |e| f

        // after
        //    1  2  3
        // 1  a  b  c
        // 2 |d  e| f

        let mut component = TableComponent::default();
        component.headers = vec!["1", "2", "3"].iter().map(|h| h.to_string()).collect();
        component.rows = vec![
            vec!["a", "b", "c"].iter().map(|h| h.to_string()).collect(),
            vec!["d", "e", "f"].iter().map(|h| h.to_string()).collect(),
        ];
        component.selected_row.select(Some(1));
        component.selected_column = 1;
        component.expand_selected_area_x(false);
        assert_eq!(component.selection_area_corner, Some((0, 1)));
        assert_eq!(component.selected_cells(), Some("d,e".to_string()));
    }

    #[test]
    fn test_expand_selected_area_x_right() {
        // before
        //    1  2  3
        // 1  a  b  c
        // 2  d |e| f

        // after
        //    1  2  3
        // 1  a  b  c
        // 2  d |e  f|

        let mut component = TableComponent::default();
        component.headers = vec!["1", "2", "3"].iter().map(|h| h.to_string()).collect();
        component.rows = vec![
            vec!["a", "b", "c"].iter().map(|h| h.to_string()).collect(),
            vec!["d", "e", "f"].iter().map(|h| h.to_string()).collect(),
        ];
        component.selected_row.select(Some(1));
        component.selected_column = 1;
        component.expand_selected_area_x(true);
        assert_eq!(component.selection_area_corner, Some((2, 1)));
        assert_eq!(component.selected_cells(), Some("e,f".to_string()));
    }

    #[test]
    fn test_expand_selected_area_y_up() {
        // before
        //    1  2  3
        // 1  a  b  c
        // 2  d |e| f

        // after
        //    1  2  3
        // 1  a |b| c
        // 2  d |e| f

        let mut component = TableComponent::default();
        component.rows = vec![
            vec!["a", "b", "c"].iter().map(|h| h.to_string()).collect(),
            vec!["d", "e", "f"].iter().map(|h| h.to_string()).collect(),
        ];
        component.selected_row.select(Some(1));
        component.selected_column = 1;
        component.expand_selected_area_y(false);
        assert_eq!(component.selection_area_corner, Some((1, 0)));
        assert_eq!(component.selected_cells(), Some("b\ne".to_string()));
    }

    #[test]
    fn test_expand_selected_area_y_down() {
        // before
        //    1  2  3
        // 1  a |b| c
        // 2  d  e  f

        // after
        //    1  2  3
        // 1  a |b| c
        // 2  d |e| f

        let mut component = TableComponent::default();
        component.rows = vec![
            vec!["a", "b", "c"].iter().map(|h| h.to_string()).collect(),
            vec!["d", "e", "f"].iter().map(|h| h.to_string()).collect(),
        ];
        component.selected_row.select(Some(0));
        component.selected_column = 1;
        component.expand_selected_area_y(true);
        assert_eq!(component.selection_area_corner, Some((1, 1)));
        assert_eq!(component.selected_cells(), Some("b\ne".to_string()));
    }

    #[test]
    fn test_is_number_column() {
        let mut component = TableComponent::default();
        component.headers = vec!["1", "2", "3"].iter().map(|h| h.to_string()).collect();
        component.rows = vec![
            vec!["a", "b", "c"].iter().map(|h| h.to_string()).collect(),
            vec!["d", "e", "f"].iter().map(|h| h.to_string()).collect(),
        ];
        component.selected_row.select(Some(0));
        assert!(component.is_number_column(0, 0));
        assert!(!component.is_number_column(0, 1));
    }

    #[test]
    fn test_selected_cell_when_one_cell_selected() {
        //    1  2 3
        // 1 |a| b c
        // 2  d  e f

        let mut component = TableComponent::default();
        component.headers = vec!["1", "2", "3"].iter().map(|h| h.to_string()).collect();
        component.rows = vec![
            vec!["a", "b", "c"].iter().map(|h| h.to_string()).collect(),
            vec!["d", "e", "f"].iter().map(|h| h.to_string()).collect(),
        ];
        component.selected_row.select(Some(0));
        assert_eq!(component.selected_cells(), Some("a".to_string()));
    }

    #[test]
    fn test_selected_cell_when_multiple_cells_selected() {
        //    1  2  3
        // 1 |a  b| c
        // 2 |d  e| f

        let mut component = TableComponent::default();
        component.headers = vec!["1", "2", "3"].iter().map(|h| h.to_string()).collect();
        component.rows = vec![
            vec!["a", "b", "c"].iter().map(|h| h.to_string()).collect(),
            vec!["d", "e", "f"].iter().map(|h| h.to_string()).collect(),
        ];
        component.selected_row.select(Some(0));
        component.selection_area_corner = Some((1, 1));
        assert_eq!(component.selected_cells(), Some("a,b\nd,e".to_string()));
    }

    #[test]
    fn test_is_selected_cell_when_one_cell_selected() {
        //    1  2 3
        // 1 |a| b c
        // 2  d  e f

        let mut component = TableComponent::default();
        component.headers = vec!["1", "2", "3"].iter().map(|h| h.to_string()).collect();
        component.rows = vec![
            vec!["a", "b", "c"].iter().map(|h| h.to_string()).collect(),
            vec!["d", "e", "f"].iter().map(|h| h.to_string()).collect(),
        ];
        component.selected_row.select(Some(0));
        // a
        assert!(component.is_selected_cell(0, 1, 1));
        // d
        assert!(!component.is_selected_cell(1, 1, 1));
        // e
        assert!(!component.is_selected_cell(1, 2, 1));
    }

    #[test]
    fn test_is_selected_cell_when_multiple_cells_selected() {
        //    1  2  3
        // 1 |a  b| c
        // 2 |d  e| f

        let mut component = TableComponent::default();
        component.headers = vec!["1", "2", "3"].iter().map(|h| h.to_string()).collect();
        component.rows = vec![
            vec!["a", "b", "c"].iter().map(|h| h.to_string()).collect(),
            vec!["d", "e", "f"].iter().map(|h| h.to_string()).collect(),
        ];
        component.selected_row.select(Some(0));
        component.selection_area_corner = Some((1, 1));
        // a
        assert!(component.is_selected_cell(0, 1, 1));
        // b
        assert!(component.is_selected_cell(0, 2, 1));
        // d
        assert!(component.is_selected_cell(1, 1, 1));
        // e
        assert!(component.is_selected_cell(1, 2, 1));
        // f
        assert!(!component.is_selected_cell(1, 3, 1));
    }

    #[test]
    fn test_calculate_cell_widths() {
        let mut component = TableComponent::default();
        component.headers = vec!["1", "2", "3"].iter().map(|h| h.to_string()).collect();
        component.rows = vec![
            vec!["aaaaa", "bbbbb", "ccccc"]
                .iter()
                .map(|h| h.to_string())
                .collect(),
            vec!["d", "e", "f"].iter().map(|h| h.to_string()).collect(),
        ];
        let (selected_column_index, headers, rows, constraints) =
            component.calculate_cell_widths(10);
        assert_eq!(selected_column_index, 1);
        assert_eq!(headers, vec!["", "1", "2"]);
        assert_eq!(rows, vec![vec!["1", "aaaaa", "bbbbb"], vec!["2", "d", "e"]]);
        assert_eq!(
            constraints,
            vec![
                Constraint::Length(1),
                Constraint::Length(5),
                Constraint::Min(10),
            ]
        );

        let (selected_column_index, headers, rows, constraints) =
            component.calculate_cell_widths(20);
        assert_eq!(selected_column_index, 1);
        assert_eq!(headers, vec!["", "1", "2", "3"]);
        assert_eq!(
            rows,
            vec![
                vec!["1", "aaaaa", "bbbbb", "ccccc"],
                vec!["2", "d", "e", "f"]
            ]
        );
        assert_eq!(
            constraints,
            vec![
                Constraint::Length(1),
                Constraint::Length(5),
                Constraint::Length(5),
                Constraint::Min(10),
            ]
        );

        let mut component = TableComponent::default();
        component.headers = vec!["1", "2", "3"].iter().map(|h| h.to_string()).collect();
        component.rows = vec![
            vec!["aaaaa", "bbbbb", "ccccc"]
                .iter()
                .map(|h| h.to_string())
                .collect(),
            vec!["dddddddddd", "e", "f"]
                .iter()
                .map(|h| h.to_string())
                .collect(),
        ];

        let (selected_column_index, headers, rows, constraints) =
            component.calculate_cell_widths(20);
        assert_eq!(selected_column_index, 1);
        assert_eq!(headers, vec!["", "1", "2", "3"]);
        assert_eq!(
            rows,
            vec![
                vec!["1", "aaaaa", "bbbbb", "ccccc"],
                vec!["2", "dddddddddd", "e", "f"]
            ]
        );
        assert_eq!(
            constraints,
            vec![
                Constraint::Length(1),
                Constraint::Length(10),
                Constraint::Length(5),
                Constraint::Min(10),
            ]
        );
    }
}
