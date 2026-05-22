#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginState {
    pub format_version: u32,
    pub payload: Vec<u8>,
}

impl PluginState {
    pub fn empty(format_version: u32) -> Self {
        Self {
            format_version,
            payload: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    UnsupportedVersion { found: u32, supported: u32 },
    CorruptPayload,
}
