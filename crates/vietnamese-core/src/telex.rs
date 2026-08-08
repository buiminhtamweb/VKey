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
                if tone == Some(candidate) {
                    tone = None;
                    output.push(character);
                } else {
                    tone = Some(candidate);
                }
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
    if unicode::shape_of(*previous) == Some(Shape::Stroke) {
        *previous = unicode::strip_shape(*previous);
        false
    } else if unicode::is_plain(*previous, 'd') {
        let is_upper = previous.is_uppercase() || character.is_uppercase();
        let char_to_shape = if is_upper {
            previous.to_ascii_uppercase()
        } else {
            previous.to_ascii_lowercase()
        };
        *previous = unicode::apply_shape(char_to_shape, Shape::Stroke).expect("d supports stroke");
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

    let target = output
        .iter()
        .rposition(|&c| unicode::plain_base(c) == Some(base));
    if let Some(index) = target {
        let c = output[index];
        if unicode::shape_of(c) == Some(Shape::Circumflex) {
            output[index] = unicode::strip_shape(c);
            false
        } else if unicode::is_plain(c, base) {
            let is_upper = c.is_uppercase() || character.is_uppercase();
            let char_to_shape = if is_upper {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            };
            if let Some(shaped) = unicode::apply_shape(char_to_shape, Shape::Circumflex) {
                output[index] = shaped;
                true
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    }
}

fn try_w_shape(output: &mut [char], character: char) -> bool {
    if !character.eq_ignore_ascii_case(&'w') {
        return false;
    }

    // 1. Check for "uo" and "ua" sequences
    let mut found_uo = None;
    let mut found_ua = None;
    if output.len() >= 2 {
        for i in 0..output.len() - 1 {
            let base_i = unicode::plain_base(output[i]);
            let base_next = unicode::plain_base(output[i + 1]);
            if base_i == Some('u') && base_next == Some('o') {
                found_uo = Some(i);
                break;
            } else if base_i == Some('u') && base_next == Some('a') {
                found_ua = Some(i);
                break;
            }
        }
    }

    if let Some(i) = found_uo {
        let u_char = output[i];
        let o_char = output[i + 1];
        if unicode::shape_of(u_char) == Some(Shape::Horn)
            || unicode::shape_of(o_char) == Some(Shape::Horn)
        {
            output[i] = unicode::strip_shape(u_char);
            output[i + 1] = unicode::strip_shape(o_char);
            false
        } else {
            output[i] = unicode::apply_shape(u_char, Shape::Horn).unwrap();
            output[i + 1] = unicode::apply_shape(o_char, Shape::Horn).unwrap();
            true
        }
    } else if let Some(i) = found_ua {
        let u_char = output[i];
        if unicode::shape_of(u_char) == Some(Shape::Horn) {
            output[i] = unicode::strip_shape(u_char);
            false
        } else {
            output[i] = unicode::apply_shape(u_char, Shape::Horn).unwrap();
            true
        }
    } else {
        // 2. Look for single vowel to modify ('u', 'o', or 'a')
        let target = output
            .iter()
            .rposition(|&c| matches!(unicode::plain_base(c), Some('u' | 'o' | 'a')));
        if let Some(index) = target {
            let c = output[index];
            let base = unicode::plain_base(c).unwrap();
            let shape = if base == 'a' {
                Shape::Breve
            } else {
                Shape::Horn
            };
            if unicode::shape_of(c) == Some(shape) {
                output[index] = unicode::strip_shape(c);
                false
            } else if unicode::is_plain(c, base) {
                let is_upper = c.is_uppercase() || character.is_uppercase();
                let char_to_shape = if is_upper {
                    c.to_ascii_uppercase()
                } else {
                    c.to_ascii_lowercase()
                };
                if let Some(shaped) = unicode::apply_shape(char_to_shape, shape) {
                    output[index] = shaped;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    }
}
