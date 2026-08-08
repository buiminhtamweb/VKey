//! Stateless Telex transformation of one composition buffer.

use crate::{
    tone::{Tone, TonePlacement},
    unicode::{self, Shape},
    word,
};

pub fn transform(input: &str, smart_tone: bool) -> String {
    let mut output = Vec::with_capacity(input.chars().count());
    let mut tone = None;

    for character in input.chars() {
        if let Some(candidate) = Tone::from_telex(character) {
            if word::contains_vowel(&output) {
                tone = Some(candidate);
                continue;
            }
        }

        if try_stroke_d(&mut output, character)
            || try_circumflex(&mut output, character)
            || try_w_shape(&mut output, character)
        {
            continue;
        }
        output.push(character);
    }

    if let Some(tone) = tone {
        TonePlacement::new(smart_tone).apply(&mut output, tone);
    }
    unicode::normalize_nfc(&output.into_iter().collect::<String>())
}

fn try_stroke_d(output: &mut [char], character: char) -> bool {
    if !character.eq_ignore_ascii_case(&'d') {
        return false;
    }
    let Some(previous) = output.last_mut() else {
        return false;
    };
    if unicode::is_plain(*previous, 'd') {
        *previous = unicode::apply_shape(*previous, Shape::Stroke).expect("d supports stroke");
        true
    } else {
        false
    }
}

fn try_circumflex(output: &mut [char], character: char) -> bool {
    let base = character.to_ascii_lowercase();
    if !matches!(base, 'a' | 'e' | 'o') {
        return false;
    }
    let Some(previous) = output.last_mut() else {
        return false;
    };
    if unicode::is_plain(*previous, base) {
        *previous = unicode::apply_shape(*previous, Shape::Circumflex)
            .expect("a, e and o support circumflex");
        true
    } else {
        false
    }
}

fn try_w_shape(output: &mut [char], character: char) -> bool {
    if !character.eq_ignore_ascii_case(&'w') {
        return false;
    }

    if output.len() >= 2 {
        let previous = output.len() - 2;
        let last = output.len() - 1;
        if unicode::is_plain(output[previous], 'u') && unicode::is_plain(output[last], 'o') {
            output[previous] =
                unicode::apply_shape(output[previous], Shape::Horn).expect("u supports horn");
            output[last] =
                unicode::apply_shape(output[last], Shape::Horn).expect("o supports horn");
            return true;
        }
    }

    let Some(previous) = output.last_mut() else {
        return false;
    };
    let shape = match unicode::plain_base(*previous) {
        Some('a') if unicode::is_plain(*previous, 'a') => Shape::Breve,
        Some('o') if unicode::is_plain(*previous, 'o') => Shape::Horn,
        Some('u') if unicode::is_plain(*previous, 'u') => Shape::Horn,
        _ => return false,
    };
    *previous = unicode::apply_shape(*previous, shape).expect("validated Telex w shape");
    true
}
