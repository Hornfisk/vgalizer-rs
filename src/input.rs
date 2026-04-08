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
    // DJ name text input
    ToggleTextInput,
    // Per-effect parameter editor
    ToggleParamEditor,
    ParamEditUp,
    ParamEditDown,
    ParamEditLeft(bool),  // bool = fast (Shift held)
    ParamEditRight(bool),
    ParamEditConfirm,
    ParamEditCancel,
    // Autopilot scene duration nudges (`[` / `]`)
    SceneDurationDown,
    SceneDurationUp,
    // Effects enable/disable menu
    ToggleEffectsMenu,
    EffectsMenuUp,
    EffectsMenuDown,
    EffectsMenuToggle,
    EffectsMenuConfirm,
    EffectsMenuCancel,
    // Global settings overlay (G)
    ToggleGlobalSettings,
    GlobalSettingsUp,
    GlobalSettingsDown,
    GlobalSettingsLeft(bool),  // bool = fast (Shift held)
    GlobalSettingsRight(bool),
    GlobalSettingsConfirm,
    GlobalSettingsCancel,
    // Unified vje-style overlay (V) — deep cross-effect param editor
    // with a viz-shrink preview. See src/text/vje_overlay.rs.
    ToggleVjeOverlay,
    VjeUp,
    VjeDown,
    VjeLeft(bool),   // bool = fast (Shift held)
    VjeRight(bool),
    VjeTab,
    VjeFocusSwap,    // Space: toggle focus between effect list and params
    VjeEnter,
    VjeEsc,
    VjeReset,
    VjeToggleDisable,
}

pub struct InputHandler {
    pub picker_open: bool,
    pub text_input_open: bool,
    pub param_editor_open: bool,
    pub effects_menu_open: bool,
    pub global_settings_open: bool,
    pub vje_open: bool,
    pub shift_held: bool,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            picker_open: false,
            text_input_open: false,
            param_editor_open: false,
            effects_menu_open: false,
            global_settings_open: false,
            vje_open: false,
            shift_held: false,
        }
    }

    pub fn handle(&self, event: &KeyEvent) -> Option<Action> {
        if event.state != ElementState::Pressed {
            return None;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return None;
        };

        // While the text input is open, the app routes raw KeyEvents
        // directly to the text-input state. Don't interpret any keys here.
        if self.text_input_open {
            return None;
        }

        // A always toggles the picker regardless of mode
        if key == KeyCode::KeyA {
            return Some(Action::ToggleAudioPicker);
        }

        // Unified vje overlay consumes nav keys exclusively while open.
        // Ordering: checked before the three single-purpose overlays below
        // so V can coexist with E/M/G without key-conflict ambiguity.
        if self.vje_open {
            return match key {
                KeyCode::ArrowUp    => Some(Action::VjeUp),
                KeyCode::ArrowDown  => Some(Action::VjeDown),
                KeyCode::ArrowLeft  => Some(Action::VjeLeft(self.shift_held)),
                KeyCode::ArrowRight => Some(Action::VjeRight(self.shift_held)),
                KeyCode::Tab        => Some(Action::VjeTab),
                KeyCode::Space      => Some(Action::VjeFocusSwap),
                KeyCode::Enter | KeyCode::NumpadEnter => Some(Action::VjeEnter),
                KeyCode::Escape     => Some(Action::VjeEsc),
                KeyCode::KeyR       => Some(Action::VjeReset),
                KeyCode::KeyX       => Some(Action::VjeToggleDisable),
                KeyCode::KeyV       => Some(Action::ToggleVjeOverlay),
                _ => None,
            };
        }

        // Effects menu consumes nav keys exclusively while open. Plain
        // arrows move the cursor; Shift+arrows nudge the autopilot scene
        // duration so the user can tune both without leaving the menu.
        if self.effects_menu_open {
            return match key {
                KeyCode::ArrowUp if self.shift_held => Some(Action::SceneDurationUp),
                KeyCode::ArrowDown if self.shift_held => Some(Action::SceneDurationDown),
                KeyCode::ArrowUp => Some(Action::EffectsMenuUp),
                KeyCode::ArrowDown => Some(Action::EffectsMenuDown),
                KeyCode::Space => Some(Action::EffectsMenuToggle),
                KeyCode::Enter | KeyCode::NumpadEnter => Some(Action::EffectsMenuConfirm),
                KeyCode::Escape => Some(Action::EffectsMenuCancel),
                KeyCode::KeyM => Some(Action::ToggleEffectsMenu),
                _ => None,
            };
        }

        // Global settings overlay consumes nav keys exclusively while open.
        if self.global_settings_open {
            return match key {
                KeyCode::ArrowUp => Some(Action::GlobalSettingsUp),
                KeyCode::ArrowDown => Some(Action::GlobalSettingsDown),
                KeyCode::ArrowLeft => Some(Action::GlobalSettingsLeft(self.shift_held)),
                KeyCode::ArrowRight => Some(Action::GlobalSettingsRight(self.shift_held)),
                KeyCode::Enter | KeyCode::NumpadEnter => Some(Action::GlobalSettingsConfirm),
                KeyCode::Escape => Some(Action::GlobalSettingsCancel),
                KeyCode::KeyG => Some(Action::ToggleGlobalSettings),
                _ => None,
            };
        }

        // Param editor consumes nav keys exclusively while open.
        if self.param_editor_open {
            return match key {
                KeyCode::ArrowUp => Some(Action::ParamEditUp),
                KeyCode::ArrowDown => Some(Action::ParamEditDown),
                KeyCode::ArrowLeft => Some(Action::ParamEditLeft(self.shift_held)),
                KeyCode::ArrowRight => Some(Action::ParamEditRight(self.shift_held)),
                KeyCode::Enter | KeyCode::NumpadEnter => Some(Action::ParamEditConfirm),
                KeyCode::Escape => Some(Action::ParamEditCancel),
                KeyCode::KeyE => Some(Action::ToggleParamEditor),
                _ => None,
            };
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
            // Arrows are the universal "nudge" gesture. Plain arrows tune
            // sensitivity (the most-touched continuous knob); Shift+arrows
            // tune the autopilot scene duration. This avoids any
            // bracket/symbol keys that aren't reachable on ISO Nordic
            // layouts without AltGr.
            KeyCode::ArrowUp if self.shift_held => Some(Action::SceneDurationUp),
            KeyCode::ArrowDown if self.shift_held => Some(Action::SceneDurationDown),
            KeyCode::ArrowUp => Some(Action::SensitivityUp),
            KeyCode::ArrowDown => Some(Action::SensitivityDown),
            KeyCode::KeyP => Some(Action::CyclePostMode),
            KeyCode::KeyH => Some(Action::ToggleHelp),
            KeyCode::KeyF => Some(Action::ToggleFullscreen),
            KeyCode::KeyW => Some(Action::ToggleWindowed),
            KeyCode::KeyT => Some(Action::ToggleTextInput),
            KeyCode::KeyE => Some(Action::ToggleParamEditor),
            KeyCode::KeyM => Some(Action::ToggleEffectsMenu),
            KeyCode::KeyG => Some(Action::ToggleGlobalSettings),
            KeyCode::KeyV => Some(Action::ToggleVjeOverlay),
            KeyCode::KeyQ | KeyCode::Escape => Some(Action::Quit),
            _ => None,
        }
    }
}
