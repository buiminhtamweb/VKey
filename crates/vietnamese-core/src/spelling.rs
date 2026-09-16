//! Vietnamese spelling and phonotactics validation.
//!
//! Used to detect when a sequence of keystrokes cannot form a valid Vietnamese
//! word (e.g. English words such as "test", "post", "filter", "format", "simple")
//! and automatically restore the original keystrokes.

use crate::{tone::Tone, unicode, word};

/// Valid Vietnamese single initial consonants.
const VALID_INITIAL_SINGLE: &[char] = &[
    'b', 'c', 'd', 'đ', 'g', 'h', 'k', 'l', 'm', 'n', 'p', 'q', 'r', 's', 't', 'v', 'x',
];

/// Valid Vietnamese multi-letter initial consonant clusters.
const VALID_INITIAL_CLUSTERS: &[&str] = &[
    "ch", "gh", "gi", "kh", "nh", "ng", "ngh", "ph", "qu", "th", "tr",
];

/// Valid Vietnamese final consonants (coda).
const VALID_FINAL_CONSONANTS: &[&str] = &["", "c", "ch", "m", "n", "ng", "nh", "p", "t"];

/// Checks whether a rendered word conforms to valid Vietnamese syllable phonotactics.
///
/// Returns `true` if the word is a valid Vietnamese syllable (or an incomplete
/// syllable that can still form one). Returns `false` if the word violates
/// Vietnamese orthography rules (meaning it is an English or foreign word).
pub fn is_valid_vietnamese_syllable(word_str: &str) -> bool {
    let lower = word_str.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();

    if chars.is_empty() {
        return true;
    }

    // 1. If the word contains non-Vietnamese letters, it is not Vietnamese.
    // 'f', 'j', 'w', 'z' cannot exist in a finished Vietnamese syllable.
    if chars.iter().any(|&c| matches!(c, 'f' | 'j' | 'w' | 'z')) {
        return false;
    }

    // 2. Find the vowel nucleus (range of vowels).
    // Handle special cases where 'u' or 'i' acts as part of the initial consonant:
    // - "qu" followed by another vowel (e.g. "quang", "quốc", "qua", "quê")
    // - "gi" followed by another vowel (e.g. "gió", "giang", "giúp")
    let (v_start, v_end) = if chars.len() >= 3
        && chars[0] == 'q'
        && unicode::plain_base(chars[1]) == Some('u')
        && word::is_vowel(chars[2])
    {
        let first = 2;
        let last = chars.iter().rposition(|&c| word::is_vowel(c)).unwrap_or(2);
        (first, last)
    } else if chars.len() >= 3
        && chars[0] == 'g'
        && unicode::plain_base(chars[1]) == Some('i')
        && word::is_vowel(chars[2])
    {
        let first = 2;
        let last = chars.iter().rposition(|&c| word::is_vowel(c)).unwrap_or(2);
        (first, last)
    } else {
        let first_vowel_idx = chars.iter().position(|&c| word::is_vowel(c));
        let last_vowel_idx = chars.iter().rposition(|&c| word::is_vowel(c));

        match (first_vowel_idx, last_vowel_idx) {
            (Some(start), Some(end)) => (start, end),
            (None, None) => {
                // Consonant-only word (e.g. "str", "b", "ng").
                // Allow if it could be a valid initial consonant cluster.
                let initial_str: String = chars.iter().collect();
                return is_valid_initial_consonant(&initial_str);
            }
            _ => unreachable!(),
        }
    };

    // Check if there are consonants trapped between vowels (e.g. "a-n-a" in one syllable)
    for &c in &chars[v_start..=v_end] {
        if !word::is_vowel(c) {
            // A single syllable cannot have consonants inside the vowel nucleus
            return false;
        }
    }

    // 3. Validate initial consonant cluster (onsets).
    let initial_str: String = chars[..v_start].iter().collect();
    if !is_valid_initial_consonant(&initial_str) {
        return false;
    }

    // 4. Validate final consonants (coda).
    let final_str: String = chars[v_end + 1..].iter().collect();
    if !VALID_FINAL_CONSONANTS.contains(&final_str.as_str()) {
        return false;
    }

    // 5. Validate tone placement with stop consonants.
    // Syllables ending with 'c', 'ch', 'p', 't' can only carry Acute (sắc) or Dot (nặng),
    // or no tone. They CANNOT carry Grave (huyền), Hook (hỏi), or Tilde (ngã).
    if matches!(final_str.as_str(), "c" | "ch" | "p" | "t") {
        for &c in &chars[v_start..=v_end] {
            if let Some(tone) = unicode::tone_of(c) {
                if !matches!(tone, Tone::Acute | Tone::Dot) {
                    return false;
                }
            }
        }
    }

    true
}

fn is_valid_initial_consonant(consonants: &str) -> bool {
    if consonants.is_empty() {
        return true;
    }
    if consonants.chars().count() == 1 {
        let c = consonants.chars().next().unwrap();
        return VALID_INITIAL_SINGLE.contains(&c);
    }
    VALID_INITIAL_CLUSTERS.contains(&consonants)
}

/// Returns true if the word contains any Vietnamese diacritic marks (tone or vowel shape).
pub fn has_vietnamese_diacritic(text: &str) -> bool {
    text.chars()
        .any(|c| unicode::tone_of(c).is_some() || unicode::shape_of(c).is_some())
}

/// Checks if the raw keystroke sequence ends with non-Vietnamese consonant clusters
/// (e.g. English endings such as "st", "rt", "lt", "rm", "sk", "sp", "ct", "ft", "xt", "pt").
pub fn has_foreign_consonant_cluster(raw: &str) -> bool {
    let lower = raw.to_lowercase();
    const FOREIGN_FINAL_CLUSTERS: &[&str] = &[
        "st", "sk", "sp", "rt", "lt", "rm", "rn", "mp", "nd", "nt", "ct", "ft", "ld", "lk", "lp",
        "pt", "xt", "rk", "ck", "sh",
    ];
    for cluster in FOREIGN_FINAL_CLUSTERS {
        if lower.ends_with(cluster) {
            return true;
        }
    }
    false
}

/// Checks if the word is a valid Vietnamese syllable, or a valid syllable with a trailing
/// restored modifier letter (e.g. 's' in "sàngs").
pub fn is_valid_vietnamese_syllable_ignoring_trailing_modifier(word: &str) -> bool {
    if is_valid_vietnamese_syllable(word) {
        return true;
    }
    if let Some(stripped) =
        word.strip_suffix(|c: char| matches!(c.to_ascii_lowercase(), 's' | 'f' | 'r' | 'x' | 'j'))
    {
        if is_valid_vietnamese_syllable(stripped) {
            return true;
        }
    }
    false
}

/// Determines whether the typed word should be restored to the raw keystrokes.
///
/// Restores only when:
/// 1. `rendered` actually contains Vietnamese diacritics (tones or vowel shapes).
///    If `rendered` has no diacritics (e.g. cancelled with shape/tone toggle like "dd", "tama", "sangs", "uuw"),
///    it is not an accidental transformation and should not be restored to `raw`.
/// 2. AND either:
///    - `raw` ends with foreign consonant clusters (e.g. "test" -> "tét", "post" -> "pót", "cost" -> "cót"), OR
///    - `rendered` violates Vietnamese syllable phonotactics (e.g. "filtẻ", "fỏmat", "cleả", "smảt").
pub fn should_restore_raw(rendered: &str, raw: &str) -> bool {
    if !has_vietnamese_diacritic(rendered) {
        return false;
    }
    if has_foreign_consonant_cluster(raw) {
        return true;
    }
    if !is_valid_vietnamese_syllable_ignoring_trailing_modifier(rendered) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_vietnamese_syllables() {
        let valids = [
            "tiếng", "Việt", "Bùi", "bùi", "được", "hóa", "toán", "nguyễn", "thủy", "nghiêng",
            "quang", "chạy", "trăng", "hoa", "cơm", "anh", "em", "ơi", "ba", "mẹ", "ông", "bà",
            "học", "hát",
        ];
        for word in valids {
            assert!(
                is_valid_vietnamese_syllable(word),
                "Expected '{word}' to be valid Vietnamese syllable"
            );
        }
    }

    #[test]
    fn invalid_english_words_rejected() {
        let invalids = [
            "test", "post", "filter", "format", "simple", "string", "print", "clear", "create",
            "play", "text", "script", "help", "build", "task", "smart", "cost", "fast", "most",
        ];
        for word in invalids {
            assert!(
                !is_valid_vietnamese_syllable(word),
                "Expected English word '{word}' to be detected as invalid Vietnamese"
            );
        }
    }

    #[test]
    fn stop_consonants_tone_restrictions() {
        // "bác" (acute) and "bạc" (dot) are valid
        assert!(is_valid_vietnamese_syllable("bác"));
        assert!(is_valid_vietnamese_syllable("bạc"));
        // "bàc" (grave), "bảc" (hook), "bãc" (tilde) are invalid
        assert!(!is_valid_vietnamese_syllable("bàc"));
        assert!(!is_valid_vietnamese_syllable("bảc"));
        assert!(!is_valid_vietnamese_syllable("bãc"));
    }
}
