//! Tone parsing and placement, kept separate from either input method.

use crate::{unicode, word};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Acute,
    Grave,
    Hook,
    Tilde,
    Dot,
}

impl Tone {
    pub(crate) const fn combining_mark(self) -> char {
        match self {
            Self::Acute => '\u{0301}',
            Self::Grave => '\u{0300}',
            Self::Hook => '\u{0309}',
            Self::Tilde => '\u{0303}',
            Self::Dot => '\u{0323}',
        }
    }

    pub(crate) fn from_telex(character: char) -> Option<Self> {
        match character.to_ascii_lowercase() {
            's' => Some(Self::Acute),
            'f' => Some(Self::Grave),
            'r' => Some(Self::Hook),
            'x' => Some(Self::Tilde),
            'j' => Some(Self::Dot),
            _ => None,
        }
    }

    pub(crate) const fn from_vni(character: char) -> Option<Self> {
        match character {
            '1' => Some(Self::Acute),
            '2' => Some(Self::Grave),
            '3' => Some(Self::Hook),
            '4' => Some(Self::Tilde),
            '5' => Some(Self::Dot),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TonePlacement {
    smart: bool,
}

impl TonePlacement {
    pub const fn new(smart: bool) -> Self {
        Self { smart }
    }

    pub fn apply(self, characters: &mut [char], tone: Tone) {
        word::normalize_vowel_nucleus(characters, self.smart);

        for character in characters
            .iter_mut()
            .filter(|character| word::is_vowel(**character))
        {
            *character = unicode::strip_tone(*character);
        }

        if let Some(index) = self.target_index(characters) {
            characters[index] = unicode::apply_tone(characters[index], tone);
        }
    }

    fn target_index(self, characters: &[char]) -> Option<usize> {
        let mut vowels: Vec<usize> = characters
            .iter()
            .enumerate()
            .filter_map(|(index, character)| word::is_vowel(*character).then_some(index))
            .collect();

        if self.smart && vowels.len() > 1 {
            vowels.retain(|index| {
                let previous = index
                    .checked_sub(1)
                    .and_then(|i| characters.get(i))
                    .copied();
                let has_later_vowel = characters[index + 1..].iter().copied().any(word::is_vowel);
                !((unicode::plain_base(characters[*index]) == Some('u')
                    && previous.is_some_and(|c| c.eq_ignore_ascii_case(&'q'))
                    || unicode::plain_base(characters[*index]) == Some('i')
                        && previous.is_some_and(|c| c.eq_ignore_ascii_case(&'g')))
                    && has_later_vowel)
            });
        }

        if vowels.is_empty() {
            return None;
        }

        if self.smart {
            if let Some(index) = vowels
                .iter()
                .rev()
                .copied()
                .find(|index| unicode::is_shaped_vowel(characters[*index]))
            {
                return Some(index);
            }
        }

        let last_vowel = *vowels.last()?;
        let closed_syllable = characters[last_vowel + 1..]
            .iter()
            .any(|character| character.is_alphabetic() && !word::is_vowel(*character));

        if !self.smart || closed_syllable {
            Some(last_vowel)
        } else {
            Some(vowels[(vowels.len().saturating_sub(1)) / 2])
        }
    }
}
