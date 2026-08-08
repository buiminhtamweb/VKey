//! Stateless VNI transformation of one composition buffer.

use crate::{
    tone::{Tone, TonePlacement},
    unicode::{self, Shape},
    word,
};

pub fn transform(input: &str, smart_tone: bool) -> String {
    let mut output = Vec::with_capacity(input.chars().count());
    let mut tone = None;

    for character in input.chars() {
        if let Some(candidate) = Tone::from_vni(character) {
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

        if matches!(character, '6' | '7' | '8' | '9') && apply_shape_digit(&mut output, character) {
            continue;
        }
        output.push(character);
    }

    if let Some(tone) = tone {
        TonePlacement::new(smart_tone).apply(&mut output, tone);
    }
    unicode::normalize_nfc(&output.into_iter().collect::<String>())
}

fn apply_shape_digit(output: &mut [char], digit: char) -> bool {
    let shape = match digit {
        '6' => Shape::Circumflex,
        '7' => Shape::Horn,
        '8' => Shape::Breve,
        '9' => Shape::Stroke,
        _ => return false,
    };

    let target = output.iter().rposition(|character| match digit {
        '6' => {
            unicode::shape_of(*character) == Some(Shape::Circumflex)
                || (matches!(unicode::plain_base(*character), Some('a' | 'e' | 'o'))
                    && unicode::is_plain(*character, unicode::plain_base(*character).unwrap()))
        }
        '7' => {
            unicode::shape_of(*character) == Some(Shape::Horn)
                || (matches!(unicode::plain_base(*character), Some('o' | 'u'))
                    && unicode::is_plain(*character, unicode::plain_base(*character).unwrap()))
        }
        '8' => {
            unicode::shape_of(*character) == Some(Shape::Breve)
                || unicode::is_plain(*character, 'a')
        }
        '9' => {
            unicode::shape_of(*character) == Some(Shape::Stroke)
                || unicode::is_plain(*character, 'd')
        }
        _ => false,
    });

    let Some(index) = target else {
        return false;
    };

    let c = output[index];
    if unicode::shape_of(c) == Some(shape) {
        output[index] = unicode::strip_shape(c);
        false
    } else {
        if let Some(shaped) = unicode::apply_shape(c, shape) {
            output[index] = shaped;
            true
        } else {
            false
        }
    }
}
