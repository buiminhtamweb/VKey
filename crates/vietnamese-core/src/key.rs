#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Character(char),
    Backspace,
    Enter,
    Escape,
    Tab,
    Space,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Shift,
    Control,
    Alt,
    Super,
    CapsLock,
    NumLock,
    F(u8),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Press,
    Release,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
    pub state: KeyState,
}

impl KeyEvent {
    pub const fn character(character: char) -> Self {
        Self {
            key: Key::Character(character),
            modifiers: Modifiers {
                shift: character.is_ascii_uppercase(),
                ctrl: false,
                alt: false,
                super_key: false,
                caps_lock: false,
                num_lock: false,
            },
            state: KeyState::Press,
        }
    }

    pub const fn press(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers {
                ctrl: false,
                alt: false,
                shift: false,
                super_key: false,
                caps_lock: false,
                num_lock: false,
            },
            state: KeyState::Press,
        }
    }

    pub const fn release(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers {
                ctrl: false,
                alt: false,
                shift: false,
                super_key: false,
                caps_lock: false,
                num_lock: false,
            },
            state: KeyState::Release,
        }
    }

    pub const fn is_pressed(self) -> bool {
        matches!(self.state, KeyState::Press)
    }
}
