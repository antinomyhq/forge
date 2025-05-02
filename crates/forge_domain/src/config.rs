use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct ForgeConfig {
    #[serde(default)]
    pub updates: UpdateConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConfig {
    #[serde(default = "default_check_frequency")]
    pub check_frequency: String,

    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
}


impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            check_frequency: default_check_frequency(),
            auto_update: default_auto_update(),
        }
    }
}

fn default_check_frequency() -> String {
    "daily".into()
}

fn default_auto_update() -> bool {
    false
}
