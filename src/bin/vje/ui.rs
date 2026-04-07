//! ratatui draw functions for vje.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};
use vgalizer::effects::EFFECT_NAMES;

use crate::edit::read_param;
use crate::state::{
    global_value_string, AppState, EffectsFocus, GlobalRow, Tab, GLOBAL_ROWS,
};

pub fn draw(f: &mut Frame, state: &AppState) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Min(5),    // main body
            Constraint::Length(1), // status bar
            Constraint::Length(1), // help hint
        ])
        .split(area);

    draw_title(f, chunks[0], state);
    match state.tab {
        Tab::Effects => draw_effects_tab(f, chunks[1], state),
        Tab::Globals => draw_globals_tab(f, chunks[1], state),
    }
    draw_status(f, chunks[2], state);
    draw_help_line(f, chunks[3], state);

    if state.help_open {
        draw_help_overlay(f, area);
    }
}

fn draw_title(f: &mut Frame, area: Rect, state: &AppState) {
    let dirty = if state.is_dirty() { " • MODIFIED" } else { "" };
    let tab_label = match state.tab {
        Tab::Effects => "[Effects]  Globals ",
        Tab::Globals => " Effects  [Globals]",
    };
    let title = Line::from(vec![
        Span::styled("vje", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(tab_label, Style::default().fg(Color::Cyan)),
        Span::styled(dirty, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(title), area);
}

fn draw_status(f: &mut Frame, area: Rect, state: &AppState) {
    let style = if state.is_dirty() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(Paragraph::new(state.status.as_str()).style(style), area);
}

fn draw_help_line(f: &mut Frame, area: Rect, state: &AppState) {
    let hint = if state.dj_name_edit.is_some() {
        "type dj_name • Enter=save • Esc=cancel"
    } else {
        match state.tab {
            Tab::Effects => match state.effects_focus {
                EffectsFocus::List   => "↑↓/jk move • Enter/→/l open params • X=disable • Tab=globals • ?=help • q=quit",
                EffectsFocus::Params => "↑↓ row • ←→ nudge (Shift=×10) • Enter=commit • R=reset • Esc/Bksp=back • ?=help",
            },
            Tab::Globals => "↑↓ row • ←→ nudge (Shift=×10) • Enter=commit / text • R=reset • Tab=effects • ?=help • q=quit",
        }
    };
    f.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

// === Effects tab ============================================================

fn draw_effects_tab(f: &mut Frame, area: Rect, state: &AppState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(24), Constraint::Min(20)])
        .split(area);

    // Left: effect list
    let disabled = state
        .config
        .disabled_effects
        .as_ref()
        .cloned()
        .unwrap_or_default();
    let items: Vec<ListItem> = EFFECT_NAMES
        .iter()
        .map(|name| {
            let is_disabled = disabled.iter().any(|d| d == name);
            let marker = if is_disabled { "✗ " } else { "  " };
            let style = if is_disabled {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(format!("{}{}", marker, name)).style(style)
        })
        .collect();

    let focused = matches!(state.effects_focus, EffectsFocus::List);
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(" effects ")
        .border_style(border_style(focused));
    let list = List::new(items)
        .block(list_block)
        .highlight_style(
            Style::default()
                .bg(Color::Magenta)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    let mut list_state = ListState::default();
    list_state.select(Some(state.effect_cursor));
    f.render_stateful_widget(list, cols[0], &mut list_state);

    // Right: param table for current effect
    let effect = state.current_effect();
    let params = state.current_effect_params();
    let focused_r = matches!(state.effects_focus, EffectsFocus::Params);

    let rows: Vec<Row> = params
        .iter()
        .map(|def| {
            let cur = read_param(&state.config, effect, def.name, def.default);
            let bar = render_bar(cur, def.min, def.max, 16);
            let changed = (cur - def.default).abs() > f32::EPSILON;
            let name_style = if changed {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };
            Row::new(vec![
                Cell::from(def.name).style(name_style),
                Cell::from(format!("{:.3}", cur)),
                Cell::from(bar),
                Cell::from(format!("[{:.2}..{:.2}]", def.min, def.max))
                    .style(Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let table_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" params: {} ", effect))
        .border_style(border_style(focused_r));

    if params.is_empty() {
        let p = Paragraph::new("(no editable params)")
            .block(table_block)
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: false });
        f.render_widget(p, cols[1]);
    } else {
        let widths = [
            Constraint::Length(14),
            Constraint::Length(8),
            Constraint::Length(20),
            Constraint::Length(16),
        ];
        let table = Table::new(rows, widths).block(table_block).row_highlight_style(
            Style::default()
                .bg(Color::Magenta)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );
        let mut table_state = TableState::default();
        if focused_r {
            table_state.select(Some(state.param_cursor));
        }
        f.render_stateful_widget(table, cols[1], &mut table_state);
    }
}

// === Globals tab ============================================================

fn draw_globals_tab(f: &mut Frame, area: Rect, state: &AppState) {
    let rows: Vec<Row> = GLOBAL_ROWS
        .iter()
        .map(|row: &GlobalRow| {
            let value = global_value_string(&state.config, row.kind);
            let label_style = if row.editable {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let value_style = if row.editable {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Row::new(vec![
                Cell::from(row.label).style(label_style),
                Cell::from(value).style(value_style),
            ])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" globals ")
        .border_style(border_style(true));

    let widths = [Constraint::Length(24), Constraint::Min(20)];
    let table = Table::new(rows, widths).block(block).row_highlight_style(
        Style::default()
            .bg(Color::Magenta)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    );
    let mut table_state = TableState::default();
    table_state.select(Some(state.global_cursor));
    f.render_stateful_widget(table, area, &mut table_state);

    // If editing dj_name, overlay a one-line text input at the bottom of area.
    if let Some(buf) = &state.dj_name_edit {
        let input_area = Rect {
            x: area.x + 2,
            y: area.y + area.height.saturating_sub(3),
            width: area.width.saturating_sub(4),
            height: 1,
        };
        let text = format!("dj_name> {}█", buf);
        f.render_widget(
            Paragraph::new(text).style(Style::default().fg(Color::Yellow).bg(Color::Black)),
            input_area,
        );
    }
}

// === Help overlay ===========================================================

fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let w = area.width.min(60);
    let h = area.height.min(18);
    let x = (area.width - w) / 2;
    let y = (area.height - h) / 2;
    let rect = Rect { x, y, width: w, height: h };

    let text = vec![
        Line::from("vje — keymap").fg(Color::Magenta),
        Line::from(""),
        Line::from("Tab / Shift+Tab   switch Effects/Globals"),
        Line::from("↑ ↓ / j k         move cursor"),
        Line::from("→ ← / l h         nudge value (Shift = ×10)"),
        Line::from("Enter             (list) open params  (row) commit / text-edit"),
        Line::from("Esc / Backspace   (in params) back to effect list"),
        Line::from("R                 reset hovered to default"),
        Line::from("X                 toggle disable for current effect"),
        Line::from("Esc               (in list/globals) revert uncommitted edits"),
        Line::from("q / Ctrl+C        quit (warns if dirty)"),
        Line::from("?                 toggle this help"),
        Line::from(""),
        Line::from("edits land in ~/.config/vgalizer/config.json").fg(Color::DarkGray),
        Line::from("running vgalizer picks up changes within ~100ms").fg(Color::DarkGray),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" help ")
        .border_style(Style::default().fg(Color::Magenta));
    let p = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    f.render_widget(ratatui::widgets::Clear, rect);
    f.render_widget(p, rect);
}

// === Helpers ================================================================

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn render_bar(value: f32, min: f32, max: f32, width: usize) -> String {
    if max <= min || width == 0 {
        return String::new();
    }
    let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
    let filled = (t * width as f32).round() as usize;
    let mut s = String::with_capacity(width + 2);
    s.push('[');
    for i in 0..width {
        s.push(if i < filled { '█' } else { '·' });
    }
    s.push(']');
    s
}
