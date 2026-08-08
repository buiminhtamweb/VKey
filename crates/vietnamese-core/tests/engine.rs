use unicode_segmentation::UnicodeSegmentation;
use vietnamese_core::{
    EngineAction, EngineConfig, InputEngine, InputMethod, Key, KeyEvent, KeyState, Modifiers,
};

fn apply_action(output: &mut String, action: EngineAction, passthrough: Option<char>) {
    match action {
        EngineAction::Consume => {}
        EngineAction::PassThrough => {
            if let Some(character) = passthrough {
                output.push(character);
            }
        }
        EngineAction::Commit(text) => output.push_str(&text),
        EngineAction::Replace {
            delete_graphemes,
            text,
        } => {
            for _ in 0..delete_graphemes {
                let new_len = output
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(index, _)| index);
                output.truncate(new_len);
            }
            output.push_str(&text);
        }
    }
}

fn type_text_with_config(input: &str, config: EngineConfig) -> String {
    let mut engine = InputEngine::new(config);
    let mut output = String::new();
    for character in input.chars() {
        let action = engine.process_key(KeyEvent::character(character));
        apply_action(&mut output, action, Some(character));
    }
    output
}

fn type_text(input: &str) -> String {
    type_text_with_config(input, EngineConfig::default())
}

fn type_vni(input: &str) -> String {
    type_text_with_config(
        input,
        EngineConfig {
            input_method: InputMethod::Vni,
            ..EngineConfig::default()
        },
    )
}

#[test]
fn required_telex_words() {
    for (input, expected) in [
        ("tieengs", "tiếng"),
        ("Vieejt", "Việt"),
        ("Tieesng", "Tiếng"),
        ("dd", "đ"),
        ("Dd", "Đ"),
        ("DD", "Đ"),
        ("aw", "ă"),
        ("aa", "â"),
        ("ee", "ê"),
        ("oo", "ô"),
        ("ow", "ơ"),
        ("uw", "ư"),
    ] {
        assert_eq!(type_text(input), expected, "input: {input}");
    }
}

#[test]
fn required_telex_tones() {
    for (input, expected) in [
        ("as", "á"),
        ("af", "à"),
        ("ar", "ả"),
        ("ax", "ã"),
        ("aj", "ạ"),
        ("AS", "Á"),
        ("AF", "À"),
        ("hoa", "hoa"),
        ("hoaf", "hòa"),
        ("hoas", "hóa"),
        ("hoar", "hỏa"),
        ("hoax", "hõa"),
        ("hoaj", "họa"),
        ("toans", "toán"),
        ("nguyenx", "nguyễn"),
    ] {
        assert_eq!(type_text(input), expected, "input: {input}");
    }
}

#[test]
fn required_sentence() {
    assert_eq!(type_text("tieengs Vieejt"), "tiếng Việt");
}

#[test]
fn vni_shapes_and_tones() {
    for (input, expected) in [
        ("tie6ng1", "tiếng"),
        ("Vie65t", "Việt"),
        ("a8", "ă"),
        ("o6", "ô"),
        ("o7", "ơ"),
        ("u7", "ư"),
        ("d9", "đ"),
        ("a1", "á"),
        ("a2", "à"),
        ("a3", "ả"),
        ("a4", "ã"),
        ("a5", "ạ"),
    ] {
        assert_eq!(type_vni(input), expected, "input: {input}");
    }
}

#[test]
fn backspace_removes_composed_graphemes() {
    let mut engine = InputEngine::new(EngineConfig::default());
    let mut output = String::new();
    for character in "tieengs".chars() {
        let action = engine.process_key(KeyEvent::character(character));
        apply_action(&mut output, action, Some(character));
    }
    assert_eq!(output, "tiếng");

    for expected in ["tiến", "tiế", "ti"] {
        let action = engine.process_key(KeyEvent::press(Key::Backspace));
        apply_action(&mut output, action, None);
        assert_eq!(output, expected);
    }
}

#[test]
fn restore_key_returns_original_keystrokes() {
    let mut engine = InputEngine::new(EngineConfig::default());
    let mut output = String::new();
    for character in "tieengsz".chars() {
        let action = engine.process_key(KeyEvent::character(character));
        apply_action(&mut output, action, Some(character));
    }
    assert_eq!(output, "tieengs");
}

#[test]
fn shortcuts_and_disabled_engine_pass_through() {
    let mut engine = InputEngine::new(EngineConfig::default());
    let shortcut = KeyEvent {
        key: Key::Character('c'),
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
        state: KeyState::Press,
    };
    assert_eq!(engine.process_key(shortcut), EngineAction::PassThrough);

    let disabled = EngineConfig {
        enabled: false,
        ..EngineConfig::default()
    };
    assert_eq!(type_text_with_config("tieengs", disabled), "tieengs");
}

#[test]
fn runtime_enable_toggle_resets_composition() {
    let mut engine = InputEngine::new(EngineConfig::default());
    engine.process_key(KeyEvent::character('a'));
    engine.set_enabled(false);

    assert_eq!(engine.current_text(), "");
    assert_eq!(
        engine.process_key(KeyEvent::character('w')),
        EngineAction::PassThrough
    );
}

#[test]
fn malformed_modifiers_are_preserved() {
    assert_eq!(type_text("123z"), "123z");
    assert_eq!(type_vni("999"), "999");
}

#[test]
fn switching_input_method_resets_composition() {
    let mut engine = InputEngine::new(EngineConfig::default());
    for character in "aw".chars() {
        engine.process_key(KeyEvent::character(character));
    }
    assert_eq!(engine.current_text(), "ă");

    engine.set_input_method(InputMethod::Vni);
    assert_eq!(engine.current_text(), "");
    for character in "a8".chars() {
        engine.process_key(KeyEvent::character(character));
    }
    assert_eq!(engine.current_text(), "ă");
}

#[test]
fn output_is_unicode_nfc() {
    use unicode_normalization::UnicodeNormalization;

    for output in [type_text("tieengs"), type_text("Vieejt"), type_vni("o65")] {
        assert_eq!(output, output.nfc().collect::<String>());
        assert_eq!(output.chars().count(), output.graphemes(true).count());
    }
}
