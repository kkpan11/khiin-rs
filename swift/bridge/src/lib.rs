use std::ffi::c_void;
use std::path::PathBuf;

use khiin::Engine;
use khiin_protos::command::Command;
use khiin_protos::command::CommandType;
use khiin_protos::command::Request;
use khiin_protos::config::AppConfig;
use khiin_protos::config::AppInputMode;
use khiin_protos::config::BoolValue;
use khiin_settings::SettingsManager;
use protobuf::Message;

#[swift_bridge::bridge]
mod ffi {
    extern "Rust" {
        type EngineBridge;

        #[swift_bridge(associated_to = EngineBridge)]
        fn new(db_filename: String) -> Option<EngineBridge>;

        #[swift_bridge(swift_name = "sendCommand")]
        fn send_command(&self, cmd_input: &[u8]) -> Option<Vec<u8>>;

        #[swift_bridge(swift_name = "loadSettings")]
        fn load_settings(&self, setting_filename: String) -> Option<Vec<u8>>;
    }
}

pub struct EngineBridge {
    engine_ptr: *mut c_void,
}

impl EngineBridge {
    fn new(db_filename: String) -> Option<Self> {
        if let Some(engine) = khiin::Engine::new(&db_filename) {
            let ptr = Box::into_raw(Box::new(engine));
            let controller = EngineBridge {
                engine_ptr: ptr as *mut c_void,
            };
            return Some(controller);
        }

        None
    }

    fn send_command(&self, cmd_input: &[u8]) -> Option<Vec<u8>> {
        let engine: &mut Engine =
            unsafe { &mut *(self.engine_ptr as *mut Engine) };

        engine.send_command_bytes(cmd_input).ok()
    }

    fn load_settings(&self, setting_filename: String) -> Option<Vec<u8>> {
        let engine: &mut Engine =
            unsafe { &mut *(self.engine_ptr as *mut Engine) };
        let path = PathBuf::from(setting_filename);
        let settings = SettingsManager::load_from_file(&path).settings;

        let input_mode = match settings.input_settings.input_mode.as_str() {
            "continuous" => AppInputMode::CONTINUOUS,
            "classic" => AppInputMode::CLASSIC,
            "manual" => AppInputMode::MANUAL,
            _ => AppInputMode::CONTINUOUS, // Default value if input mode is not recognized
        };

        let mut config: AppConfig = AppConfig::new();
        config.input_mode = input_mode.into();
        // set telex enabled to rust protobuf boolvalue true
        let mut telex_enabled = BoolValue::new();
        telex_enabled.value = settings.input_settings.tone_mode == "telex";
        config.telex_enabled = Some(telex_enabled).into();

        let mut req = Request::new();
        req.type_ = CommandType::CMD_SET_CONFIG.into();
        req.config = Some(config.clone()).into();

        let mut cmd = Command::new();
        cmd.request = Some(req).into();
        if let Ok(bytes) = cmd.write_to_bytes() {
            let _ = engine.send_command_bytes(&bytes);
        }
        config.write_to_bytes().ok()
    }
}
