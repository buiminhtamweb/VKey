use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputMethod {
    Telex,
    Vni,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub enabled: bool,
    pub input_method: InputMethod,
    pub smart_tone: bool,
    pub restore_typing: bool,
    pub restore_key: char,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            input_method: InputMethod::Telex,
            smart_tone: true,
            restore_typing: true,
            restore_key: 'z',
        }
    }
}
