use crate::tone::Tone;
use crate::unicode;

/// Map a Vietnamese char (including shapes and tones) to its plain ASCII character.
fn char_to_plain_ascii(c: char) -> char {
    match c.to_ascii_lowercase() {
        'a' | 'à' | 'á' | 'ả' | 'ã' | 'ạ' | 'ă' | 'ằ' | 'ắ' | 'ẳ' | 'ẵ' | 'ặ' | 'â' | 'ầ' | 'ấ'
        | 'ẩ' | 'ẫ' | 'ậ' => 'a',
        'e' | 'è' | 'é' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ề' | 'ế' | 'ể' | 'ễ' | 'ệ' => {
            'e'
        }
        'i' | 'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' => 'i',
        'o' | 'ò' | 'ó' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ồ' | 'ố' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ờ' | 'ớ'
        | 'ở' | 'ỡ' | 'ợ' => 'o',
        'u' | 'ù' | 'ú' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ừ' | 'ứ' | 'ử' | 'ữ' | 'ự' => {
            'u'
        }
        'y' | 'ỳ' | 'ý' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
        'đ' | 'd' => 'd',
        other => other,
    }
}

/// Convert a whole word to its plain ASCII representation.
fn to_plain_ascii(word: &str) -> String {
    word.chars().map(char_to_plain_ascii).collect()
}

/// Extract the active tone of a word.
fn extract_tone(word: &str) -> Option<Tone> {
    word.chars().find_map(unicode::tone_of)
}

/// Check if a word is a valid Vietnamese syllable.
pub fn is_valid_vietnamese_syllable(word: &str) -> bool {
    let lower_word = word.to_lowercase();
    if lower_word.is_empty() || lower_word.chars().any(|c| !c.is_alphabetic()) {
        return true;
    }

    let has_vowel = lower_word
        .chars()
        .any(|c| matches!(char_to_plain_ascii(c), 'a' | 'e' | 'i' | 'o' | 'u' | 'y'));
    if !has_vowel {
        return true;
    }

    let plain = to_plain_ascii(&lower_word);
    let tone = extract_tone(&lower_word);

    let mut chars = plain.chars().peekable();

    // 1. Initial Consonant
    let mut initial = String::new();
    let is_consonant = |c: char| -> bool {
        matches!(
            c,
            'b' | 'c'
                | 'd'
                | 'g'
                | 'h'
                | 'k'
                | 'l'
                | 'm'
                | 'n'
                | 'p'
                | 'q'
                | 'r'
                | 's'
                | 't'
                | 'v'
                | 'x'
        )
    };

    while let Some(&c) = chars.peek() {
        if is_consonant(c) {
            initial.push(c);
            chars.next();
        } else {
            break;
        }
    }

    // 2. Vowels
    let mut vowels = String::new();
    let is_vowel = |c: char| -> bool { matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y') };

    while let Some(&c) = chars.peek() {
        if is_vowel(c) {
            vowels.push(c);
            chars.next();
        } else {
            break;
        }
    }

    // 3. Final Consonant
    let mut final_consonant = String::new();
    for c in chars {
        if is_consonant(c) {
            final_consonant.push(c);
        } else {
            return false;
        }
    }

    // --- Validation Rules ---

    let valid_initials = [
        "", "b", "c", "ch", "d", "g", "gh", "gi", "h", "k", "kh", "l", "m", "n", "nh", "ng", "ngh",
        "p", "ph", "q", "r", "s", "t", "th", "tr", "v", "x",
    ];
    if !valid_initials.contains(&initial.as_str()) {
        return false;
    }

    let valid_finals = ["", "c", "ch", "m", "n", "nh", "ng", "p", "t"];
    if !valid_finals.contains(&final_consonant.as_str()) {
        return false;
    }

    if vowels.is_empty() {
        return false;
    }

    let valid_vowels = [
        "a", "e", "i", "o", "u", "y", "ai", "ao", "au", "ay", "eo", "eu", "ey", "ia", "ie", "iu",
        "oa", "oe", "oi", "oo", "ua", "ue", "ui", "uo", "uy", "ieu", "oai", "oao", "oay", "oeo",
        "uay", "uoi", "uou", "uye", "uyeu",
    ];
    if !valid_vowels.contains(&vowels.as_str()) {
        return false;
    }

    if !initial.is_empty() {
        let first_vowel = vowels.chars().next().unwrap();
        match initial.as_str() {
            "k" | "gh" | "ngh" if !matches!(first_vowel, 'i' | 'e' | 'y') => {
                return false;
            }
            "c" | "g" | "ng" if matches!(first_vowel, 'i' | 'e' | 'y') => {
                return false;
            }
            "q" if first_vowel != 'u' || vowels.len() < 2 => {
                return false;
            }
            _ => {}
        }
    }

    if !final_consonant.is_empty() {
        match final_consonant.as_str() {
            "ch" | "nh" => {
                if !matches!(vowels.as_str(), "a" | "e" | "i" | "oa" | "uy") {
                    return false;
                }
                if final_consonant == "ch" {
                    if let Some(t) = tone {
                        if !matches!(t, Tone::Acute | Tone::Dot) {
                            return false;
                        }
                    }
                }
            }
            "c" | "p" | "t" => {
                if let Some(t) = tone {
                    if !matches!(t, Tone::Acute | Tone::Dot) {
                        return false;
                    }
                }
            }
            "g" | "ng" => {
                if matches!(vowels.as_str(), "i" | "e" | "y") {
                    return false;
                }
            }
            _ => {}
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_syllables() {
        assert!(is_valid_vietnamese_syllable("sáng"));
        assert!(is_valid_vietnamese_syllable("tuyết"));
        assert!(is_valid_vietnamese_syllable("huyên"));
        assert!(is_valid_vietnamese_syllable("ngành"));
        assert!(is_valid_vietnamese_syllable("chuyên"));
    }

    #[test]
    fn test_invalid_syllables() {
        assert!(!is_valid_vietnamese_syllable("sángs"));
        assert!(!is_valid_vietnamese_syllable("tuyềt"));
        assert!(!is_valid_vietnamese_syllable("nghành"));
    }
}
