#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Character(char),
    Backspace,
    Enter,
    Tab,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Delete,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
    pub pressed: bool,
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
            },
            pressed: true,
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
            },
            pressed: true,
        }
    }
}
