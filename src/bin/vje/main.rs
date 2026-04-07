//! vje — vj edit. Standalone TUI param editor for vgalizer.
//!
//! Writes to ~/.config/vgalizer/config.json via the existing atomic
//! write_xdg_fields helper. The running vgalizer's notify watcher picks up
//! the rename within ~100ms — no IPC, no restart.

mod edit;
mod state;
mod ui;

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use vgalizer::config::load_merged;
use vgalizer::effects::EFFECT_NAMES;

use crate::edit::{
    commit_to_disk, nudge_effect_param, nudge_global, reset_effect_param, reset_global,
    toggle_disabled_effect,
};
use crate::state::{AppState, EffectsFocus, GlobalKind, Tab, GLOBAL_ROWS};

fn main() -> io::Result<()> {
    // Load via the same path vgalizer uses at startup. "config.json" is the
    // seed shipped in the repo; load_merged will overlay the XDG file.
    let config = load_merged("config.json");
    let mut app = AppState::new(config);

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, &mut app);

    // Teardown (always, even on error)
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut AppState,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            handle_key(app, key);
            if app.quit {
                return Ok(());
            }
        }
    }
}

fn handle_key(app: &mut AppState, key: KeyEvent) {
    // Text-input mode for dj_name takes precedence.
    if let Some(buf) = app.dj_name_edit.as_mut() {
        match key.code {
            KeyCode::Esc => {
                app.dj_name_edit = None;
                app.status = "dj_name edit cancelled".into();
            }
            KeyCode::Enter => {
                app.config.dj_name = buf.clone();
                app.dirty_fields.insert("dj_name");
                app.dj_name_edit = None;
                app.status = "dj_name updated (Enter again to commit)".into();
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) => {
                buf.push(c);
            }
            _ => {}
        }
        return;
    }

    // Help overlay: any key closes it.
    if app.help_open {
        app.help_open = false;
        return;
    }

    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Char('q') => {
            if app.is_dirty() {
                app.status = "unsaved edits — Esc to revert, Enter to commit, Ctrl+C to force quit".into();
            } else {
                app.quit = true;
            }
        }
        KeyCode::Char('c') if ctrl => {
            app.quit = true;
        }
        KeyCode::Char('?') => {
            app.help_open = true;
        }
        KeyCode::Esc => {
            // Context-aware back: in Params focus, first Esc pops back to
            // the effect list. Otherwise Esc reverts uncommitted edits.
            if matches!(app.tab, Tab::Effects) && matches!(app.effects_focus, EffectsFocus::Params) {
                app.effects_focus = EffectsFocus::List;
                app.status = "back to effect list".into();
            } else {
                app.revert();
            }
        }
        KeyCode::Tab | KeyCode::BackTab => {
            app.tab = match app.tab {
                Tab::Effects => Tab::Globals,
                Tab::Globals => Tab::Effects,
            };
        }
        _ => match app.tab {
            Tab::Effects => handle_effects_key(app, key, shift),
            Tab::Globals => handle_globals_key(app, key, shift),
        },
    }
}

fn handle_effects_key(app: &mut AppState, key: KeyEvent, shift: bool) {
    match app.effects_focus {
        EffectsFocus::List => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if app.effect_cursor > 0 {
                    app.effect_cursor -= 1;
                    app.param_cursor = 0;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.effect_cursor + 1 < EFFECT_NAMES.len() {
                    app.effect_cursor += 1;
                    app.param_cursor = 0;
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                app.effects_focus = EffectsFocus::Params;
                app.param_cursor = 0;
            }
            KeyCode::Char('x') | KeyCode::Char('X') => toggle_disabled_effect(app),
            _ => {}
        },
        EffectsFocus::Params => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if app.param_cursor > 0 {
                    app.param_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let n = app.current_effect_params().len();
                if n > 0 && app.param_cursor + 1 < n {
                    app.param_cursor += 1;
                }
            }
            KeyCode::Left => nudge_effect_param(app, -1, shift),
            KeyCode::Right => nudge_effect_param(app, 1, shift),
            KeyCode::Char('h') => nudge_effect_param(app, -1, shift),
            KeyCode::Char('l') => nudge_effect_param(app, 1, shift),
            KeyCode::Char('r') | KeyCode::Char('R') => reset_effect_param(app),
            KeyCode::Char('x') | KeyCode::Char('X') => toggle_disabled_effect(app),
            KeyCode::Enter => commit_to_disk(app),
            KeyCode::Backspace => {
                app.effects_focus = EffectsFocus::List;
            }
            _ => {}
        },
    }
}

fn handle_globals_key(app: &mut AppState, key: KeyEvent, shift: bool) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if app.global_cursor > 0 {
                app.global_cursor -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.global_cursor + 1 < GLOBAL_ROWS.len() {
                app.global_cursor += 1;
            }
        }
        KeyCode::Left => nudge_global(app, -1, shift),
        KeyCode::Right => nudge_global(app, 1, shift),
        KeyCode::Char('h') => nudge_global(app, -1, shift),
        KeyCode::Char('l') => nudge_global(app, 1, shift),
        KeyCode::Char('r') | KeyCode::Char('R') => reset_global(app),
        KeyCode::Enter => {
            // If hovered row is dj_name, enter text-input mode.
            if let Some(row) = GLOBAL_ROWS.get(app.global_cursor) {
                if matches!(row.kind, GlobalKind::DjName) && row.editable {
                    app.dj_name_edit = Some(app.config.dj_name.clone());
                    app.status = "editing dj_name — Enter to accept, Esc to cancel".into();
                    return;
                }
            }
            commit_to_disk(app);
        }
        _ => {}
    }
}
