use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    NextEffect,
    JumpTo(usize),
    SensitivityUp,
    SensitivityDown,
    CyclePostMode,
    ToggleHelp,
    ToggleFullscreen,
    ToggleWindowed,
    Quit,
}

pub struct InputHandler;

impl InputHandler {
    pub fn new() -> Self {
        Self
    }

    pub fn handle(&self, event: &KeyEvent) -> Option<Action> {
        if event.state != ElementState::Pressed {
            return None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return None;
        };
        match key {
            KeyCode::Space => Some(Action::NextEffect),
            KeyCode::Digit1 => Some(Action::JumpTo(1)),
            KeyCode::Digit2 => Some(Action::JumpTo(2)),
            KeyCode::Digit3 => Some(Action::JumpTo(3)),
            KeyCode::Digit4 => Some(Action::JumpTo(4)),
            KeyCode::Digit5 => Some(Action::JumpTo(5)),
            KeyCode::Digit6 => Some(Action::JumpTo(6)),
            KeyCode::Digit7 => Some(Action::JumpTo(7)),
            KeyCode::Digit8 => Some(Action::JumpTo(8)),
            KeyCode::Digit9 => Some(Action::JumpTo(9)),
            KeyCode::Equal | KeyCode::NumpadAdd => Some(Action::SensitivityUp),
            KeyCode::Minus | KeyCode::NumpadSubtract => Some(Action::SensitivityDown),
            KeyCode::KeyP => Some(Action::CyclePostMode),
            KeyCode::KeyH => Some(Action::ToggleHelp),
            KeyCode::KeyF => Some(Action::ToggleFullscreen),
            KeyCode::KeyW => Some(Action::ToggleWindowed),
            KeyCode::KeyQ | KeyCode::Escape => Some(Action::Quit),
            _ => None,
        }
    }
}
