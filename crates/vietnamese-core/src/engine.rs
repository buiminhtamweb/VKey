use unicode_segmentation::UnicodeSegmentation;

use crate::{
    composition::CompositionBuffer,
    config::{EngineConfig, InputMethod},
    key::{Key, KeyEvent},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineAction {
    Consume,
    PassThrough,
    Commit(String),
    Replace {
        delete_graphemes: usize,
        text: String,
    },
}

pub struct InputEngine {
    config: EngineConfig,
    composition: CompositionBuffer,
}

impl InputEngine {
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            composition: CompositionBuffer::default(),
        }
    }

    pub fn process_key(&mut self, event: KeyEvent) -> EngineAction {
        if !event.is_pressed() {
            return EngineAction::PassThrough;
        }
        if !self.config.enabled
            || event.modifiers.ctrl
            || event.modifiers.alt
            || event.modifiers.super_key
        {
            self.reset();
            return EngineAction::PassThrough;
        }

        match event.key {
            Key::Character(character) if is_delimiter(character, self.config.input_method) => {
                self.reset();
                EngineAction::PassThrough
            }
            key if is_composition_boundary(key) => {
                self.reset();
                EngineAction::PassThrough
            }
            Key::Character(character) => {
                let old = self.composition.rendered().to_owned();
                self.composition.push(
                    character,
                    self.config.input_method,
                    self.config.smart_tone,
                    self.config.restore_typing,
                );
                let new = self.composition.rendered();

                let old_conv = crate::charset::convert(&old, self.config.charset);
                let new_conv = crate::charset::convert(new, self.config.charset);

                if new_conv.strip_prefix(&old_conv) == Some(character.encode_utf8(&mut [0; 4])) {
                    EngineAction::PassThrough
                } else {
                    replacement(&old_conv, &new_conv)
                }
            }
            _ => {
                self.reset();
                EngineAction::PassThrough
            }
        }
    }

    pub fn reset(&mut self) {
        self.composition.clear();
    }

    pub fn current_text(&self) -> &str {
        self.composition.rendered()
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: EngineConfig) {
        self.config = config;
        self.reset();
    }

    pub fn set_input_method(&mut self, input_method: InputMethod) {
        self.config.input_method = input_method;
        self.reset();
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
        self.reset();
    }
}

fn is_delimiter(character: char, method: InputMethod) -> bool {
    if character.is_alphabetic() {
        return false;
    }
    method != InputMethod::Vni || !matches!(character, '1'..='9')
}

fn is_composition_boundary(key: Key) -> bool {
    matches!(
        key,
        Key::Backspace
            | Key::Delete
            | Key::Enter
            | Key::Escape
            | Key::Tab
            | Key::Space
            | Key::Left
            | Key::Right
            | Key::Up
            | Key::Down
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
    )
}

fn replacement(old: &str, new: &str) -> EngineAction {
    let mut old_iter = old.grapheme_indices(true).peekable();
    let mut new_iter = new.grapheme_indices(true).peekable();
    let mut new_suffix_byte = 0;

    while let (Some((_, old_grapheme)), Some((new_index, new_grapheme))) =
        (old_iter.peek().copied(), new_iter.peek().copied())
    {
        if old_grapheme != new_grapheme {
            new_suffix_byte = new_index;
            break;
        }
        old_iter.next();
        new_iter.next();
        new_suffix_byte = new_index + new_grapheme.len();
    }

    let delete_graphemes = old_iter.count();
    let text = new[new_suffix_byte..].to_owned();
    if delete_graphemes == 0 && text.is_empty() {
        EngineAction::Consume
    } else {
        EngineAction::Replace {
            delete_graphemes,
            text,
        }
    }
}
