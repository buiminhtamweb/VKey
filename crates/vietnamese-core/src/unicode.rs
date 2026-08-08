//! Unicode operations used by both input methods.

use unicode_normalization::UnicodeNormalization;

use crate::tone::Tone;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    Breve,
    Circumflex,
    Horn,
    Stroke,
}

const TONE_MARKS: [char; 5] = ['\u{0301}', '\u{0300}', '\u{0309}', '\u{0303}', '\u{0323}'];

pub fn normalize_nfc(input: &str) -> String {
    input.nfc().collect()
}

pub(crate) fn tone_of(character: char) -> Option<Tone> {
    character.nfd().find_map(|mark| match mark {
        '\u{0301}' => Some(Tone::Acute),
        '\u{0300}' => Some(Tone::Grave),
        '\u{0309}' => Some(Tone::Hook),
        '\u{0303}' => Some(Tone::Tilde),
        '\u{0323}' => Some(Tone::Dot),
        _ => None,
    })
}

pub(crate) fn strip_tone(character: char) -> char {
    let decomposed: String = character
        .to_string()
        .nfd()
        .filter(|mark| !TONE_MARKS.contains(mark))
        .collect();
    single_nfc_char(&decomposed).unwrap_or(character)
}

pub(crate) fn apply_tone(character: char, tone: Tone) -> char {
    let mut decomposed: String = strip_tone(character).to_string().nfd().collect();
    decomposed.push(tone.combining_mark());
    single_nfc_char(&decomposed).unwrap_or(character)
}

pub(crate) fn apply_shape(character: char, shape: Shape) -> Option<char> {
    let tone = tone_of(character);
    let uppercase = character.is_uppercase();
    let base = plain_base(character)?;
    let shaped = match (base, shape, uppercase) {
        ('a', Shape::Breve, false) => 'ă',
        ('a', Shape::Breve, true) => 'Ă',
        ('a', Shape::Circumflex, false) => 'â',
        ('a', Shape::Circumflex, true) => 'Â',
        ('e', Shape::Circumflex, false) => 'ê',
        ('e', Shape::Circumflex, true) => 'Ê',
        ('o', Shape::Circumflex, false) => 'ô',
        ('o', Shape::Circumflex, true) => 'Ô',
        ('o', Shape::Horn, false) => 'ơ',
        ('o', Shape::Horn, true) => 'Ơ',
        ('u', Shape::Horn, false) => 'ư',
        ('u', Shape::Horn, true) => 'Ư',
        ('d', Shape::Stroke, false) => 'đ',
        ('d', Shape::Stroke, true) => 'Đ',
        _ => return None,
    };
    Some(tone.map_or(shaped, |tone| apply_tone(shaped, tone)))
}

pub(crate) fn plain_base(character: char) -> Option<char> {
    match strip_tone(character).to_lowercase().next()? {
        'a' | 'ă' | 'â' => Some('a'),
        'e' | 'ê' => Some('e'),
        'i' => Some('i'),
        'o' | 'ô' | 'ơ' => Some('o'),
        'u' | 'ư' => Some('u'),
        'y' => Some('y'),
        'd' | 'đ' => Some('d'),
        _ => None,
    }
}

pub(crate) fn shape_of(character: char) -> Option<Shape> {
    let stripped = strip_tone(character).to_lowercase().next()?;
    match stripped {
        'ă' => Some(Shape::Breve),
        'â' | 'ê' | 'ô' => Some(Shape::Circumflex),
        'ơ' | 'ư' => Some(Shape::Horn),
        'đ' => Some(Shape::Stroke),
        _ => None,
    }
}

pub(crate) fn strip_shape(character: char) -> char {
    let tone = tone_of(character);
    let uppercase = character.is_uppercase();
    if let Some(base) = plain_base(character) {
        let plain = if uppercase {
            base.to_ascii_uppercase()
        } else {
            base
        };
        tone.map_or(plain, |t| apply_tone(plain, t))
    } else {
        character
    }
}

pub(crate) fn is_plain(character: char, base: char) -> bool {
    let stripped = strip_tone(character);
    stripped.eq_ignore_ascii_case(&base) && stripped.is_ascii()
}

pub(crate) fn is_shaped_vowel(character: char) -> bool {
    matches!(
        strip_tone(character).to_lowercase().next(),
        Some('ă' | 'â' | 'ê' | 'ô' | 'ơ' | 'ư')
    )
}

fn single_nfc_char(input: &str) -> Option<char> {
    let normalized: String = input.nfc().collect();
    let mut characters = normalized.chars();
    let character = characters.next()?;
    characters.next().is_none().then_some(character)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_precomposed_vowels() {
        assert_eq!(apply_tone('ê', Tone::Acute), 'ế');
        assert_eq!(apply_tone('Ư', Tone::Dot), 'Ự');
        assert_eq!(normalize_nfc("e\u{0302}\u{0301}"), "ế");
    }

    #[test]
    fn removes_only_tone_not_vowel_shape() {
        assert_eq!(strip_tone('ệ'), 'ê');
        assert_eq!(strip_tone('Ắ'), 'Ă');
    }
}
