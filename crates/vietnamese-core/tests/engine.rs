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

fn pop_grapheme(output: &mut String) {
    let new_len = output
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(index, _)| index);
    output.truncate(new_len);
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
        ("duocdwj", "được"),
        ("Duocdwj", "Được"),
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
fn regression_buif_is_bui_with_grave_tone() {
    assert_eq!(type_text("buif"), "bùi");
}

#[test]
fn capitalized_word_does_not_keep_an_accidental_second_uppercase_letter() {
    assert_eq!(type_text("HOcj"), "Học");
    assert_eq!(type_vni("HO5c"), "Học");
    assert_eq!(type_text("HỌc"), "Học");

    assert_eq!(type_text("HOCJ"), "HỌC");
    assert_eq!(type_vni("HO5C"), "HỌC");
    assert_eq!(type_text("HỌC"), "HỌC");
    assert_eq!(type_text("ĐắkLắk"), "ĐắkLắk");
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
fn backspace_clears_composition_and_passes_through() {
    let mut engine = InputEngine::new(EngineConfig::default());
    let mut output = String::new();
    for character in "tieengs".chars() {
        let action = engine.process_key(KeyEvent::character(character));
        apply_action(&mut output, action, Some(character));
    }
    assert_eq!(output, "tiếng");

    assert_eq!(
        engine.process_key(KeyEvent::press(Key::Backspace)),
        EngineAction::PassThrough
    );
    assert_eq!(engine.current_text(), "");
    pop_grapheme(&mut output);
    assert_eq!(output, "tiến");

    let action = engine.process_key(KeyEvent::character('s'));
    apply_action(&mut output, action, Some('s'));
    assert_eq!(output, "tiếns");
}

#[test]
fn boundary_keys_clear_composition_memory() {
    for key in [
        Key::Space,
        Key::Left,
        Key::Right,
        Key::Up,
        Key::Down,
        Key::Home,
        Key::End,
        Key::PageUp,
        Key::PageDown,
        Key::Delete,
        Key::Backspace,
    ] {
        let mut engine = InputEngine::new(EngineConfig::default());
        for character in "tam".chars() {
            engine.process_key(KeyEvent::character(character));
        }
        assert_eq!(engine.current_text(), "tam");
        assert_eq!(
            engine.process_key(KeyEvent::press(key)),
            EngineAction::PassThrough,
            "key: {key:?}"
        );
        assert_eq!(engine.current_text(), "", "key: {key:?}");
        assert_eq!(
            engine.process_key(KeyEvent::character('a')),
            EngineAction::PassThrough,
            "key: {key:?}"
        );
    }
}

#[test]
fn tone_cancellation_key_removes_tone() {
    let mut engine = InputEngine::new(EngineConfig::default());
    let mut output = String::new();
    for character in "tieengsz".chars() {
        let action = engine.process_key(KeyEvent::character(character));
        apply_action(&mut output, action, Some(character));
    }
    assert_eq!(output, "tiêng");
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

#[test]
fn test_telex_end_of_word_modifiers() {
    assert_eq!(type_text("tama"), "tâm");
    assert_eq!(type_text("tamaa"), "tama");
    assert_eq!(type_text("muoons"), "muốn");
    assert_eq!(type_text("toto"), "tôt");
    assert_eq!(type_text("totos"), "tốt");
    assert_eq!(type_text("hopos"), "hốp");
    assert_eq!(type_text("duongw"), "dương");
    assert_eq!(type_text("dduongw"), "đương");
    assert_eq!(type_text("dduongwf"), "đường");
    assert_eq!(type_text("suarw"), "sửa");
    assert_eq!(type_text("suawr"), "sửa");
}

#[test]
fn test_tone_toggling() {
    let config_telex = EngineConfig::default();

    let config_vni = EngineConfig {
        input_method: InputMethod::Vni,
        ..EngineConfig::default()
    };

    assert_eq!(type_text_with_config("sangs", config_telex.clone()), "sáng");
    assert_eq!(
        type_text_with_config("sangss", config_telex.clone()),
        "sangs"
    );
    assert_eq!(
        type_text_with_config("sangssf", config_telex.clone()),
        "sàngs"
    );
    assert_eq!(
        type_text_with_config("sangssr", config_telex.clone()),
        "sảngs"
    );
    assert_eq!(type_text_with_config("sang1", config_vni.clone()), "sáng");
    assert_eq!(type_text_with_config("sang11", config_vni.clone()), "sang1");
}

#[test]
fn test_shape_toggling() {
    assert_eq!(type_text("ooo"), "oo");
    assert_eq!(type_text("eee"), "ee");
    assert_eq!(type_text("aaa"), "aa");
    assert_eq!(type_text("ddd"), "dd");
    assert_eq!(type_vni("a66"), "a6");
}

#[test]
fn test_charsets() {
    use vietnamese_core::Charset;

    let config_tcvn3 = EngineConfig {
        charset: Charset::Tcvn3,
        ..EngineConfig::default()
    };
    assert_eq!(
        type_text_with_config("tieengs", config_tcvn3.clone()),
        "tiÕng"
    );

    let config_vni = EngineConfig {
        charset: Charset::Vni,
        ..EngineConfig::default()
    };
    assert_eq!(
        type_text_with_config("tieengs", config_vni.clone()),
        "tieáng"
    );
    assert_eq!(type_text_with_config("dduongwf", config_tcvn3), "®õêng");
    assert_eq!(type_text_with_config("dduongwf", config_vni), "ñöôøng");
}
