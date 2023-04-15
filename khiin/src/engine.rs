use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Error;
use anyhow::Result;

use protobuf::Message;

use khiin_protos::command::preedit::*;
use khiin_protos::command::*;

use crate::buffer::BufferMgr;
use crate::config::Config;
use crate::config::InputType;
use crate::data::database::Database;
use crate::data::dictionary::Dictionary;

pub struct Engine {
    db: Database,
    dict: Dictionary,
    conf: Config,
    buffer_mgr: BufferMgr,
}

impl Engine {
    pub fn new(filename: &str) -> Option<Engine> {
        let path = PathBuf::from(filename);
        if !path.exists() {
            return None;
        }

        let db = Database::new(&path).ok()?;
        let dict = Dictionary::new(&db, InputType::Numeric).ok()?;

        Some(Engine {
            db,
            dict,
            buffer_mgr: BufferMgr::new(),
            conf: Config::new(),
        })
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn dict(&self) -> &Dictionary {
        &self.dict
    }

    pub fn conf(&self) -> &Config {
        &self.conf
    }

    pub fn send_command_bytes(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        let mut cmd = Command::parse_from_bytes(bytes)?;
        let req = cmd.request.clone().unwrap();

        let res = match req.type_.enum_value_or_default() {
            CommandType::CMD_UNSPECIFIED => {
                Err(anyhow!("Command not specified"))
            },
            CommandType::CMD_SEND_KEY => self.on_send_key(req),
            CommandType::CMD_REVERT => self.on_revert(req),
            CommandType::CMD_RESET => self.on_reset(req),
            CommandType::CMD_COMMIT => self.on_commit(req),
            CommandType::CMD_SELECT_CANDIDATE => self.on_select_candidate(req),
            CommandType::CMD_FOCUS_CANDIDATE => self.on_focus_candidate(req),
            CommandType::CMD_SWITCH_INPUT_MODE => {
                self.on_switch_input_mode(req)
            },
            CommandType::CMD_PLACE_CURSOR => self.on_place_cursor(req),
            CommandType::CMD_DISABLE => self.on_disable(req),
            CommandType::CMD_ENABLE => self.on_enable(req),
            CommandType::CMD_SET_CONFIG => self.on_set_config(req),
            CommandType::CMD_TEST_SEND_KEY => self.on_test_send_key(req),
            CommandType::CMD_LIST_EMOJIS => self.on_list_emojis(req),
            CommandType::CMD_RESET_USER_DATA => self.on_reset_user_data(req),
            CommandType::CMD_SHUTDOWN => self.on_shutdown(req),
        };

        if let Ok(res) = res {
            cmd.response = Some(res).into();
        } else {
            let mut res = Response::default();
            res.error = ErrorCode::FAIL.into();
            cmd.response = Some(res).into();
        }

        cmd.write_to_bytes()
            .map_err(|_| Error::msg("Failed to write protobuf bytes"))
    }

    fn on_send_key(&mut self, req: Request) -> Result<Response> {
        match req.key_event.special_key.enum_value_or_default() {
            SpecialKey::SK_NONE => {
                let ch = ascii_char_from_i32(req.key_event.key_code);
                if let Some(ch) = ch {
                    self.buffer_mgr
                        .insert(&self.db, &self.dict, &self.conf, ch)?;
                }
            },
            SpecialKey::SK_SPACE => {},
            SpecialKey::SK_ENTER => {},
            SpecialKey::SK_ESC => {},
            SpecialKey::SK_BACKSPACE => {},
            SpecialKey::SK_TAB => {},
            SpecialKey::SK_LEFT => {},
            SpecialKey::SK_UP => {},
            SpecialKey::SK_RIGHT => {},
            SpecialKey::SK_DOWN => {},
            SpecialKey::SK_PGUP => {},
            SpecialKey::SK_PGDN => {},
            SpecialKey::SK_HOME => {},
            SpecialKey::SK_END => {},
            SpecialKey::SK_DEL => {},
        };

        let mut response = Response::default();
        response.preedit = Some(self.buffer_mgr.build_preedit()).into();
        response.edit_state = self.buffer_mgr.edit_state().into();
        response.candidate_list = Some(self.buffer_mgr.get_candidates()).into();
        Ok(response)
    }

    fn on_revert(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_reset(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_commit(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_select_candidate(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_focus_candidate(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_switch_input_mode(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_place_cursor(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_disable(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_enable(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_set_config(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_test_send_key(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_list_emojis(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_reset_user_data(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_shutdown(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }
}

fn ascii_char_from_i32(ch: i32) -> Option<char> {
    let ch = ch as u32;
    if let Some(ch) = char::from_u32(ch) {
        if ch.is_ascii_alphanumeric() {
            return Some(ch);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn it_works() {
        let engine = get_engine();
        assert!(engine.is_some());
    }

    #[test]
    fn it_handles_send_key_commands() -> Result<()> {
        let mut engine = get_engine().unwrap();
        let req = mock_send_key_request('a');
        let res = engine.on_send_key(req)?;
        let s = &res.preedit.segments;
        assert_eq!(s.len(), 1);
        assert_eq!(
            s[0].status.enum_value_or_default(),
            SegmentStatus::SS_COMPOSING
        );
        assert_eq!(s[0].value, "a".to_string());
        Ok(())
    }
}
