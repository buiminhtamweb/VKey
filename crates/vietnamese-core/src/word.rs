//! Small orthographic helpers shared by Telex and VNI.

use crate::unicode::{self, Shape};

pub(crate) fn is_vowel(character: char) -> bool {
    matches!(
        unicode::plain_base(character),
        Some('a' | 'e' | 'i' | 'o' | 'u' | 'y')
    )
}

pub(crate) fn contains_vowel(characters: &[char]) -> bool {
    characters.iter().copied().any(is_vowel)
}

pub(crate) fn normalize_vowel_nucleus(characters: &mut [char], smart: bool) {
    if !smart || characters.len() < 4 {
        return;
    }

    // In a closed "uyê" nucleus, Vietnamese orthography requires ê. This also
    // gives the requested friendly form `nguyenx -> nguyễn` without putting a
    // whole-word dictionary in the engine.
    for index in 0..characters.len().saturating_sub(2) {
        let is_uye = unicode::plain_base(characters[index]) == Some('u')
            && unicode::plain_base(characters[index + 1]) == Some('y')
            && unicode::is_plain(characters[index + 2], 'e');
        let is_closed = characters[index + 3..]
            .iter()
            .any(|character| character.is_alphabetic() && !is_vowel(*character));
        if is_uye && is_closed {
            if let Some(shaped) = unicode::apply_shape(characters[index + 2], Shape::Circumflex) {
                characters[index + 2] = shaped;
            }
        }
    }
}
