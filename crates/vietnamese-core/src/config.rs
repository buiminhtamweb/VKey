use serde::{Deserialize, Serialize};

use crate::charset::Charset;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputMethod {
    Telex,
    Vni,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ShortcutKey {
    #[default]
    CtrlShift,
    AltZ,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub enabled: bool,
    pub input_method: InputMethod,
    pub charset: Charset,
    pub smart_tone: bool,
    pub restore_typing: bool,
    pub restore_key: char,
    pub startup_with_system: bool,
    pub shortcut_key: ShortcutKey,
    pub spelling_check: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            input_method: InputMethod::Telex,
            charset: Charset::Unicode,
            smart_tone: true,
            restore_typing: true,
            restore_key: 'z',
            startup_with_system: false,
            shortcut_key: ShortcutKey::CtrlShift,
            spelling_check: true,
        }
    }
}
