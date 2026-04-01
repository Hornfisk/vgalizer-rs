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
    // Audio device picker
    ToggleAudioPicker,
    PickerUp,
    PickerDown,
    PickerJump(usize), // 1-indexed
    PickerConfirm,
    PickerCancel,
}

pub struct InputHandler {
    pub picker_open: bool,
}

impl InputHandler {
    pub fn new() -> Self {
        Self { picker_open: false }
    }

    pub fn handle(&self, event: &KeyEvent) -> Option<Action> {
        if event.state != ElementState::Pressed {
            return None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return None;
        };

        // A always toggles the picker regardless of mode
        if key == KeyCode::KeyA {
            return Some(Action::ToggleAudioPicker);
        }

        if self.picker_open {
            return match key {
                KeyCode::ArrowUp => Some(Action::PickerUp),
                KeyCode::ArrowDown => Some(Action::PickerDown),
                KeyCode::Digit1 => Some(Action::PickerJump(1)),
                KeyCode::Digit2 => Some(Action::PickerJump(2)),
                KeyCode::Digit3 => Some(Action::PickerJump(3)),
                KeyCode::Digit4 => Some(Action::PickerJump(4)),
                KeyCode::Digit5 => Some(Action::PickerJump(5)),
                KeyCode::Digit6 => Some(Action::PickerJump(6)),
                KeyCode::Digit7 => Some(Action::PickerJump(7)),
                KeyCode::Digit8 => Some(Action::PickerJump(8)),
                KeyCode::Digit9 => Some(Action::PickerJump(9)),
                KeyCode::Enter | KeyCode::NumpadEnter => Some(Action::PickerConfirm),
                KeyCode::Escape => Some(Action::PickerCancel),
                _ => None,
            };
        }

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
            KeyCode::Equal | KeyCode::NumpadAdd | KeyCode::ArrowUp => Some(Action::SensitivityUp),
            KeyCode::Minus | KeyCode::NumpadSubtract | KeyCode::ArrowDown => {
                Some(Action::SensitivityDown)
            }
            KeyCode::KeyP => Some(Action::CyclePostMode),
            KeyCode::KeyH => Some(Action::ToggleHelp),
            KeyCode::KeyF => Some(Action::ToggleFullscreen),
            KeyCode::KeyW => Some(Action::ToggleWindowed),
            KeyCode::KeyQ | KeyCode::Escape => Some(Action::Quit),
            _ => None,
        }
    }
}
