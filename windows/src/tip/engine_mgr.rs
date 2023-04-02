use protobuf::MessageField;

use windows::Win32::Foundation::E_FAIL;
use windows::core::Result;

use khiin_protos::command::Command;
use khiin_protos::command::KeyEvent;
use khiin_protos::command::Request;

use crate::tip::key_event::KeyEvent as WinKeyEvent;
use crate::winerr;

pub fn translate_key_event(input: WinKeyEvent) -> KeyEvent {
    let mut proto = KeyEvent::new();
    proto.key_code = input.keycode as i32;
    proto
}

pub struct EngineMgr {
    engine: khiin::Engine,
}

impl EngineMgr {
    pub fn new() -> Result<Self> {
        let engine = khiin::Engine::new();

        if engine.is_none() {
            return winerr!(E_FAIL);
        }

        Ok(EngineMgr {
            engine: engine.unwrap(),
        })
    }

    pub fn on_test_key(&self, _key_event: &WinKeyEvent) -> bool {
        false
    }

    pub fn on_key(&self, key_event: WinKeyEvent) -> Result<Command> {
        let key_event = translate_key_event(key_event);
        let mut req = Request::new();
        req.key_event = MessageField::some(key_event);
        let mut cmd = Command::new();
        cmd.request = MessageField::some(req);
        Ok(cmd)
    }

    pub fn test(&mut self) {
        return;
    }
}