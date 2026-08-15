use vietnamese_core::EngineAction;

use crate::{KeyboardDecision, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowId(pub u32);

pub trait TextInjector {
    fn insert_text(&mut self, text: &str) -> Result<()>;

    /// Delete `count` user-perceived characters from the current target.
    ///
    /// The X11 implementation emits one Backspace key event per grapheme. The
    /// focused client remains responsible for applying its normal Unicode
    /// grapheme deletion behavior.
    fn delete_previous_graphemes(&mut self, count: usize) -> Result<()>;

    fn replace_text(&mut self, delete_graphemes: usize, text: &str) -> Result<()> {
        if delete_graphemes > 0 {
            self.delete_previous_graphemes(delete_graphemes)?;
        }
        if text.is_empty() {
            Ok(())
        } else {
            self.insert_text(text)
        }
    }

    fn current_target(&self) -> Result<Option<WindowId>>;
}

pub fn execute_engine_action(
    injector: &mut impl TextInjector,
    action: &EngineAction,
) -> Result<()> {
    match action {
        EngineAction::PassThrough | EngineAction::Consume => Ok(()),
        EngineAction::Commit(text) if !text.is_empty() => injector.insert_text(text),
        EngineAction::Commit(_) => Ok(()),
        EngineAction::Replace {
            delete_graphemes,
            text,
        } => injector.replace_text(*delete_graphemes, text),
    }
}

#[must_use]
pub const fn decision_for(action: &EngineAction) -> KeyboardDecision {
    KeyboardDecision::from_engine_action(action)
}

#[cfg(test)]
mod tests {
    use unicode_segmentation::UnicodeSegmentation;
    use vietnamese_core::{EngineConfig, InputEngine, KeyEvent};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum MockAction {
        Insert(String),
        Delete(usize),
    }

    #[derive(Default)]
    struct MockTextInjector {
        text: String,
        actions: Vec<MockAction>,
    }

    impl TextInjector for MockTextInjector {
        fn insert_text(&mut self, text: &str) -> Result<()> {
            self.actions.push(MockAction::Insert(text.to_owned()));
            self.text.push_str(text);
            Ok(())
        }

        fn delete_previous_graphemes(&mut self, count: usize) -> Result<()> {
            self.actions.push(MockAction::Delete(count));
            for _ in 0..count {
                let new_len = self
                    .text
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(index, _)| index);
                self.text.truncate(new_len);
            }
            Ok(())
        }

        fn current_target(&self) -> Result<Option<WindowId>> {
            Ok(Some(WindowId(42)))
        }
    }

    fn type_through_pipeline(input: &str, config: EngineConfig) -> MockTextInjector {
        let mut engine = InputEngine::new(config);
        let mut injector = MockTextInjector::default();

        for character in input.chars() {
            let action = engine.process_key(KeyEvent::character(character));
            match decision_for(&action) {
                KeyboardDecision::PassThrough => injector.text.push(character),
                KeyboardDecision::Consume => {
                    execute_engine_action(&mut injector, &action).unwrap();
                }
            }
        }
        injector
    }

    #[test]
    fn telex_sentence_reaches_the_target_without_raw_keystrokes() {
        let injector = type_through_pipeline("tieengs Vieejt!", EngineConfig::default());

        assert_eq!(injector.text, "tiếng Việt!");
        assert!(injector.actions.iter().any(|action| matches!(
            action,
            MockAction::Insert(text) if text.contains('ế')
        )));
    }

    #[test]
    fn intercepted_shape_cancellation_keeps_the_trigger_character() {
        let injector = type_through_pipeline("tamaa", EngineConfig::default());

        assert_eq!(injector.text, "tama");
        assert!(injector.actions.windows(2).any(|actions| matches!(
            actions,
            [MockAction::Delete(2), MockAction::Insert(text)] if text == "ama"
        )));
    }

    #[test]
    fn intercepted_fast_muons_keeps_both_vowels() {
        let injector = type_through_pipeline("muoons", EngineConfig::default());

        assert_eq!(injector.text, "muốn");
    }

    #[test]
    fn intercepted_delayed_stroke_d_rewrites_the_syllable_start() {
        let injector = type_through_pipeline("duocdwj", EngineConfig::default());

        assert_eq!(injector.text, "được");
    }

    #[test]
    fn windows_regression_buif_deletes_ui_before_inserting_ui_with_tone() {
        let injector = type_through_pipeline("buif", EngineConfig::default());

        assert_eq!(injector.text, "bùi");
        assert_eq!(
            injector.actions,
            vec![MockAction::Delete(2), MockAction::Insert("ùi".to_owned())]
        );
    }

    #[test]
    fn replace_deletes_graphemes_not_utf8_bytes_or_scalars() {
        let mut injector = MockTextInjector {
            text: "tiếng".to_owned(),
            actions: Vec::new(),
        };
        execute_engine_action(
            &mut injector,
            &EngineAction::Replace {
                delete_graphemes: 1,
                text: String::new(),
            },
        )
        .unwrap();

        assert_eq!(injector.text, "tiến");
        assert_eq!(injector.actions, vec![MockAction::Delete(1)]);
    }

    #[test]
    fn disabled_engine_passes_original_keys() {
        let injector = type_through_pipeline(
            "tieengs",
            EngineConfig {
                enabled: false,
                ..EngineConfig::default()
            },
        );

        assert_eq!(injector.text, "tieengs");
        assert!(injector.actions.is_empty());
    }
}
