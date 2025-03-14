use std::fmt::Debug;
use std::path::Path;
use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Error;
use anyhow::Result;

use protobuf::Message;

use khiin_protos::command::*;
use khiin_protos::config::AppInputMode;
use khiin_protos::config::AppOutputMode;
use khiin_protos::config::AppKhinMode;
use khiin_protos::config::BoolValue;

use crate::buffer::BufferMgr;
use crate::config::Config;
use crate::config::InputMode;
use crate::config::OutputMode;
use crate::config::KhinMode;
use crate::config::ToneMode;
use crate::data::dictionary::Dictionary;
use crate::db::Database;

pub struct Engine {
    buffer_mgr: BufferMgr,
    inner: EngInner,
}

pub(crate) struct EngInner {
    pub(crate) db: Database,
    pub(crate) dict: Dictionary,
    pub(crate) conf: Config,
}

impl Engine {
    pub fn new<P>(filename: P) -> Option<Engine>
    where
        P: AsRef<Path> + Debug + Clone,
    {
        let db = Database::new(filename.clone()).ok()?;
        log::debug!("Database loaded from: {:?}", filename);
        let dict = Dictionary::new(&db, ToneMode::Numeric).ok()?;
        log::debug!("Dictionary initialized");

        Some(Engine {
            buffer_mgr: BufferMgr::new(),
            inner: EngInner {
                db,
                dict,
                conf: Config::new(),
            },
        })
    }

    pub fn send_command_bytes(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        let mut cmd = Command::parse_from_bytes(bytes)?;
        let req = cmd.request.clone().unwrap();

        let res = match req.type_.enum_value_or_default() {
            CommandType::CMD_UNSPECIFIED => {
                let mut res = Response::new();
                let mut cmd = Command::new();
                res.error = ErrorCode::FAIL.into();
                Ok(res)
            },
            CommandType::CMD_SEND_KEY => self.on_send_key(req),
            CommandType::CMD_REVERT => self.on_revert(req),
            CommandType::CMD_RESET => self.on_reset(req),
            CommandType::CMD_COMMIT => self.on_commit_all(req),
            CommandType::CMD_SELECT_CANDIDATE => self.on_select_candidate(req),
            CommandType::CMD_FOCUS_CANDIDATE => self.on_focus_candidate(req),
            CommandType::CMD_SWITCH_INPUT_MODE => {
                self.on_switch_input_mode(req)
            },
            CommandType::CMD_SWITCH_OUTPUT_MODE => {
                self.on_switch_output_mode(req)
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
        log::debug!("Engine::on_send_key");
        match req.key_event.special_key.enum_value_or_default() {
            SpecialKey::SK_NONE => {
                let ch = ascii_char_from_i32(req.key_event.key_code);
                if let Some(ch) = ch {
                    // if ch is number
                    if ch.is_ascii_digit()
                        && self.inner.conf.input_mode() == InputMode::Classic
                        && self.buffer_mgr.is_focused()
                    {
                        let idx = ch.to_digit(10).unwrap() as usize;
                        self.buffer_mgr
                            .focus_candidate_by_index(&self.inner, idx);
                        // check is candidate is action
                        if self.buffer_mgr.focused_candidate_is_action() {
                            self.buffer_mgr.expand_candidate(&self.inner)?;
                            let mut response = Response::default();
                            self.attach_buffer_data(&mut response)?;
                            return Ok(response);
                        }
                        return self.on_commit(req);
                    } else {
                        self.buffer_mgr.insert(&self.inner, ch)?;
                        if self.buffer_mgr.edit_state() == EditState::ES_EMPTY {
                            return self.on_commit_all(req);
                        }
                    }
                }
            },
            SpecialKey::SK_SPACE => {
                if (req.key_event.modifier_keys.contains(
                    &protobuf::EnumOrUnknown::from_i32(
                        ModifierKey::MODK_SHIFT as i32,
                    ),
                )) {
                    self.buffer_mgr.focus_prev_candidate(&self.inner)?;
                } else {
                    self.buffer_mgr.focus_next_candidate(&self.inner)?;
                }
                if self.buffer_mgr.edit_state() == EditState::ES_ILLEGAL
                    && self.inner.conf.input_mode() == InputMode::Classic
                {
                    return self.on_commit(req);
                }
            },
            SpecialKey::SK_ENTER => {
                return self.on_enter(req);
            },
            SpecialKey::SK_ESC => {},
            SpecialKey::SK_BACKSPACE => {
                self.buffer_mgr.pop(&self.inner)?;
            },
            SpecialKey::SK_TAB => {
                if (req.key_event.modifier_keys.contains(
                    &protobuf::EnumOrUnknown::from_i32(
                        ModifierKey::MODK_SHIFT as i32,
                    ),
                )) {
                    self.buffer_mgr.show_prev_page_candidate(&self.inner)?;
                } else {
                    self.buffer_mgr.show_next_page_candidate(&self.inner)?;
                }
            },
            SpecialKey::SK_LEFT => {},
            SpecialKey::SK_RIGHT => {},
            SpecialKey::SK_UP => {
                self.buffer_mgr.focus_prev_candidate(&self.inner)?;
            },
            SpecialKey::SK_DOWN => {
                self.buffer_mgr.focus_next_candidate(&self.inner)?;
            },
            SpecialKey::SK_PGUP => {},
            SpecialKey::SK_PGDN => {},
            SpecialKey::SK_HOME => {},
            SpecialKey::SK_END => {},
            SpecialKey::SK_DEL => {},
        };

        let mut response = Response::default();
        self.attach_buffer_data(&mut response)?;
        Ok(response)
    }

    fn on_revert(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_reset(&mut self, req: Request) -> Result<Response> {
        self.buffer_mgr.reset()?;
        Ok(Response::new())
    }

    fn on_commit_all(&mut self, req: Request) -> Result<Response> {
        let committed_text = self.buffer_mgr.commit_all(&self.inner)?;
        let mut response = Response::default();
        self.attach_buffer_data(&mut response)?;
        response.committed_text = committed_text;
        response.committed = true;
        self.buffer_mgr.reset()?;
        if let Some(ref mut p) = response.preedit.as_mut() {
            p.caret = 0;
            p.focused_caret = 0;
        }
        response.edit_state = EditState::ES_EMPTY.into();
        Ok(response)
    }

    fn on_enter(&mut self, req: Request) -> Result<Response> {
        if (self.inner.conf.input_mode() == InputMode::Classic) {
            // check is candidate is action
            if self.buffer_mgr.focused_candidate_is_action() {
                self.buffer_mgr.expand_candidate(&self.inner)?;
                let mut response = Response::default();
                self.attach_buffer_data(&mut response)?;
                return Ok(response);
            }
        }
        return self.on_commit(req);
    }

    fn on_commit(&mut self, req: Request) -> Result<Response> {
        if (self.inner.conf.input_mode() == InputMode::Classic) {
            // only classic mode need comosite remainder
            let committed_text = self
                .buffer_mgr
                .commit_candidate_and_comosite_remainder(&self.inner)?;
            let mut response = Response::default();
            self.attach_buffer_data(&mut response)?;
            response.committed_text = committed_text;
            response.committed = true;
            return Ok(response);
        }
        let mut response = Response::new();
        response.committed = true;
        self.attach_preedit(&mut response)?;
        self.buffer_mgr.reset()?;
        if let Some(ref mut p) = response.preedit.as_mut() {
            p.caret = 0;
            p.focused_caret = 0;
        }
        response.edit_state = EditState::ES_EMPTY.into();
        Ok(response)
    }

    fn on_select_candidate(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_focus_candidate(&self, req: Request) -> Result<Response> {
        Err(anyhow!("Not implemented"))
    }

    fn on_switch_input_mode(&mut self, req: Request) -> Result<Response> {
        self.buffer_mgr.reset();
        match req.config.input_mode.enum_value_or_default() {
            AppInputMode::CONTINUOUS => {
                self.inner.conf.set_input_mode(InputMode::Continuous)
            },
            AppInputMode::CLASSIC => {
                self.inner.conf.set_input_mode(InputMode::Classic)
            },
            AppInputMode::MANUAL => {
                self.inner.conf.set_input_mode(InputMode::Manual)
            },
        }
        Ok(Response::new())
    }

    fn on_switch_output_mode(&mut self, req: Request) -> Result<Response> {
        self.buffer_mgr.reset();
        match req.config.output_mode.enum_value_or_default() {
            AppOutputMode::LOMAJI => {
                self.inner.conf.set_output_mode(OutputMode::Lomaji)
            },
            AppOutputMode::HANJI => {
                self.inner.conf.set_output_mode(OutputMode::Hanji)
            },
        }
        Ok(Response::new())
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

    fn on_set_config(&mut self, req: Request) -> Result<Response> {
        self.buffer_mgr.reset();
        match req.config.input_mode.enum_value_or_default() {
            AppInputMode::CONTINUOUS => {
                self.inner.conf.set_input_mode(InputMode::Continuous)
            },
            AppInputMode::CLASSIC => {
                self.inner.conf.set_input_mode(InputMode::Classic)
            },
            AppInputMode::MANUAL => {
                self.inner.conf.set_input_mode(InputMode::Manual)
            },
        }

        match req.config.output_mode.enum_value_or_default() {
            AppOutputMode::LOMAJI => {
                self.inner.conf.set_output_mode(OutputMode::Lomaji)
            },
            AppOutputMode::HANJI => {
                self.inner.conf.set_output_mode(OutputMode::Hanji)
            },
        }

        match req.config.khin_mode.enum_value_or_default() {
            AppKhinMode::KHINLESS => {
                self.inner.conf.set_khin_mode(KhinMode::Khinless)
            },
            AppKhinMode::DOT => {
                self.inner.conf.set_khin_mode(KhinMode::Dot)
            },
            AppKhinMode::HYPHEN => {
                self.inner.conf.set_khin_mode(KhinMode::Hyphen)
            },
        }

        // let mut telex_enabled = BoolValue::new();
        if let Some(telex_enabled) = req.config.telex_enabled.as_ref() {
            if telex_enabled.value {
                self.inner.conf.set_tone_mode(ToneMode::Telex)
            } else {
                self.inner.conf.set_tone_mode(ToneMode::Numeric)
            }
        }

        Ok(Response::new())
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

    fn attach_preedit(&self, res: &mut Response) -> Result<()> {
        res.preedit = Some(self.buffer_mgr.build_preedit()).into();
        Ok(())
    }

    fn attach_candidate_list(&self, res: &mut Response) -> Result<()> {
        res.candidate_list = Some(self.buffer_mgr.get_candidates()).into();
        Ok(())
    }

    fn attach_edit_state(&self, res: &mut Response) -> Result<()> {
        res.edit_state = self.buffer_mgr.edit_state().into();
        Ok(())
    }

    fn attach_buffer_data(&self, res: &mut Response) -> Result<()> {
        self.attach_preedit(res)?;
        self.attach_candidate_list(res)?;
        self.attach_edit_state(res)
    }
}

fn ascii_char_from_i32(ch: i32) -> Option<char> {
    let ch = ch as u32;
    if let Some(ch) = char::from_u32(ch) {
        if ch.is_ascii_alphanumeric() {
            return Some(ch);
        } else if ch.is_ascii_punctuation() {
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

    #[test]
    fn it_inserts_multiple_characters() -> Result<()> {
        let mut engine = get_engine().unwrap();
        engine.on_send_key(mock_send_key_request('a'))?;
        engine.on_send_key(mock_send_key_request('a'))?;
        engine.on_send_key(mock_send_key_request('a'))?;
        engine.on_send_key(mock_send_key_request('a'))?;
        engine.on_send_key(mock_send_key_request('a'))?;
        engine.on_send_key(mock_send_key_request('a'))?;
        let res = engine.on_send_key(mock_send_key_request('a'))?;
        assert_eq!(res.preedit.segments.len(), 1);
        Ok(())
    }
}
