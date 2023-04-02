use serde::Deserialize;
use windows::core::Result;
use windows::Win32::Foundation::HMODULE;

use khiin_protos::config::AppConfig;

pub enum KhiinFile {
    Database,
    Config,
    SettingsApp,
    UserDb,
}

impl KhiinFile {
    fn reg_key(&self) -> &'static str {
        match *self {
            KhiinFile::Database => "khiin_db",
            KhiinFile::Config => "config_toml",
            KhiinFile::SettingsApp => "settings_exe",
            KhiinFile::UserDb => "user_db",
        }
    }
}

pub enum UiLanguage {
    English,
    HanloTai,
    LojiTai,
}

pub enum UiColors {
    Light,
    Dark,
}

pub enum Hotkey {
    None,
    AltBacktick,
    Shift,
    CtrlPeriod,
    CtrlBacktick,
}

#[derive(Deserialize, Default)]
pub struct WinConfig {
    pub engine: Option<EngineConfig>,
}

#[derive(Deserialize, Default)]
pub struct EngineConfig {
    pub input_mode: Option<String>,
}

pub fn load_from_file(module: HMODULE) -> Result<AppConfig> {
    Ok(AppConfig::default())
}

pub fn save_to_file(module: HMODULE, config: AppConfig) -> Result<()> {
    Ok(())
}

pub fn get_known_file(
    file: KhiinFile,
    module: Option<HMODULE>,
    filename_override: Option<&str>,
) -> String {
    "".into()
}
