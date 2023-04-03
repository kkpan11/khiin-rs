use std::cell::RefCell;
use std::cell::RefMut;
use std::rc::Rc;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::sync::RwLock;

use khiin::Engine;
use khiin_protos::command::Command;
use khiin_protos::config::AppConfig;
use khiin_protos::config::BoolValue;
use protobuf::MessageField;
use windows::core::implement;
use windows::core::AsImpl;
use windows::core::ComInterface;
use windows::core::IUnknown;
use windows::core::Result;
use windows::core::GUID;
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::UI::TextServices::CLSID_TF_CategoryMgr;
use windows::Win32::UI::TextServices::ITfCandidateListUIElement;
use windows::Win32::UI::TextServices::ITfCategoryMgr;
use windows::Win32::UI::TextServices::ITfCompartmentEventSink;
use windows::Win32::UI::TextServices::ITfCompartmentEventSink_Impl;
use windows::Win32::UI::TextServices::ITfComposition;
use windows::Win32::UI::TextServices::ITfCompositionSink;
use windows::Win32::UI::TextServices::ITfCompositionSink_Impl;
use windows::Win32::UI::TextServices::ITfContext;
use windows::Win32::UI::TextServices::ITfKeyEventSink;
use windows::Win32::UI::TextServices::ITfLangBarItemButton;
use windows::Win32::UI::TextServices::ITfTextInputProcessor;
use windows::Win32::UI::TextServices::ITfTextInputProcessorEx;
use windows::Win32::UI::TextServices::ITfTextInputProcessorEx_Impl;
use windows::Win32::UI::TextServices::ITfTextInputProcessor_Impl;
use windows::Win32::UI::TextServices::ITfThreadMgr;
use windows::Win32::UI::TextServices::ITfThreadMgrEventSink;
use windows::Win32::UI::TextServices::ITfUIElement;
use windows::Win32::UI::TextServices::GUID_COMPARTMENT_KEYBOARD_DISABLED;
use windows::Win32::UI::TextServices::GUID_COMPARTMENT_KEYBOARD_OPENCLOSE;

use crate::dll::DllModule;
use crate::reg::guids::GUID_CONFIG_CHANGED_COMPARTMENT;
use crate::reg::guids::GUID_DISPLAY_ATTRIBUTE_CONVERTED;
use crate::reg::guids::GUID_DISPLAY_ATTRIBUTE_FOCUSED;
use crate::reg::guids::GUID_DISPLAY_ATTRIBUTE_INPUT;
use crate::reg::guids::GUID_RESET_USERDATA_COMPARTMENT;
use crate::tip::candidate_list_ui::CandidateListUI;
use crate::tip::compartment::Compartment;
use crate::tip::composition_mgr::CompositionMgr;
use crate::tip::display_attributes::DisplayAttributes;
use crate::tip::engine_mgr::EngineMgr;
use crate::tip::key_event_sink::KeyEventSink;
use crate::tip::lang_bar_indicator::LangBarIndicator;
use crate::tip::preserved_key_mgr::PreservedKeyMgr;
use crate::tip::sink_mgr::SinkMgr;
use crate::tip::thread_mgr_event_sink::ThreadMgrEventSink;
use crate::ui::popup_menu::PopupMenu;
use crate::ui::window::Window;
use crate::utils::arc_lock::ArcLock;
use crate::utils::win::co_create_inproc;
use crate::winerr;

const TF_CLIENTID_NULL: u32 = 0;
const TF_INVALID_GUIDATOM: u32 = 0;

#[implement(
    ITfTextInputProcessorEx,
    ITfTextInputProcessor,
    ITfCompartmentEventSink,
    ITfCompositionSink
)]
pub struct TextService {
    // After the TextService is pinned in COM (by going `.into()`
    // the ITfTextInputProcessor), set `this` as a COM smart pointer
    // to self. All other impls that need TextService should recieve
    // a clone of `this`, and cast it to &TextService using `.as_impl()`
    this: RefCell<Option<ITfTextInputProcessor>>,

    // Given by TSF
    threadmgr: RefCell<Option<ITfThreadMgr>>,
    clientid: ArcLock<u32>,
    dwflags: ArcLock<u32>,

    // Config
    on_off_state_locked: ArcLock<bool>,
    config: Arc<RwLock<AppConfig>>,

    // Key handling
    key_event_sink: RefCell<Option<ITfKeyEventSink>>,

    // Thread mgr
    threadmgr_event_sink: RefCell<Option<ITfThreadMgrEventSink>>,
    threadmgr_event_sink_sinkmgr: RefCell<SinkMgr<ITfThreadMgrEventSink>>,

    // Compartments
    open_close_compartment: Arc<RwLock<Compartment>>,
    open_close_sinkmgr: RefCell<SinkMgr<ITfCompartmentEventSink>>,

    config_compartment: Arc<RwLock<Compartment>>,
    config_sinkmgr: RefCell<SinkMgr<ITfCompartmentEventSink>>,

    userdata_compartment: Arc<RwLock<Compartment>>,
    userdata_sinkmgr: RefCell<SinkMgr<ITfCompartmentEventSink>>,

    kbd_disabled_compartment: Arc<RwLock<Compartment>>,
    kbd_disabled_sinkmgr: RefCell<SinkMgr<ITfCompartmentEventSink>>,

    // UI elements
    disp_attrs: DisplayAttributes,
    input_attr_guidatom: ArcLock<u32>,
    converted_attr_guidatom: ArcLock<u32>,
    focused_attr_guidatom: ArcLock<u32>,
    lang_bar_indicator: RefCell<Option<ITfLangBarItemButton>>,
    preserved_key_mgr: RefCell<Option<PreservedKeyMgr>>,
    candidate_list_ui: RefCell<Option<ITfCandidateListUIElement>>,
    composition_mgr: Arc<RwLock<CompositionMgr>>,

    // Data
    engine: Arc<RwLock<EngineMgr>>,
}

// Public portion
impl TextService {
    pub fn new() -> Result<Self> {
        Ok(Self {
            this: RefCell::new(None),
            threadmgr: RefCell::new(None),
            clientid: ArcLock::new(TF_CLIENTID_NULL),
            dwflags: ArcLock::new(0),

            on_off_state_locked: ArcLock::new(false),
            config: Arc::new(RwLock::new(AppConfig::new())),

            key_event_sink: RefCell::new(None),

            threadmgr_event_sink: RefCell::new(None),
            threadmgr_event_sink_sinkmgr: RefCell::new(SinkMgr::<
                ITfThreadMgrEventSink,
            >::new()),

            open_close_compartment: Arc::new(RwLock::new(Compartment::new())),
            open_close_sinkmgr: RefCell::new(
                SinkMgr::<ITfCompartmentEventSink>::new(),
            ),

            config_compartment: Arc::new(RwLock::new(Compartment::new())),
            config_sinkmgr: RefCell::new(
                SinkMgr::<ITfCompartmentEventSink>::new(),
            ),

            userdata_compartment: Arc::new(RwLock::new(Compartment::new())),
            userdata_sinkmgr: RefCell::new(
                SinkMgr::<ITfCompartmentEventSink>::new(),
            ),

            kbd_disabled_compartment: Arc::new(RwLock::new(Compartment::new())),
            kbd_disabled_sinkmgr: RefCell::new(SinkMgr::<
                ITfCompartmentEventSink,
            >::new()),

            preserved_key_mgr: RefCell::new(None),
            disp_attrs: DisplayAttributes::new(),
            input_attr_guidatom: ArcLock::new(TF_INVALID_GUIDATOM),
            converted_attr_guidatom: ArcLock::new(TF_INVALID_GUIDATOM),
            focused_attr_guidatom: ArcLock::new(TF_INVALID_GUIDATOM),
            lang_bar_indicator: RefCell::new(None),
            candidate_list_ui: RefCell::new(None),
            composition_mgr: Arc::new(RwLock::new(CompositionMgr::new()?)),
            engine: Arc::new(RwLock::new(EngineMgr::new()?)),
        })
    }

    pub fn disp_attrs(&self) -> &DisplayAttributes {
        &self.disp_attrs
    }

    pub fn clientid(&self) -> Result<u32> {
        self.clientid.get()
    }

    pub fn enabled(&self) -> Result<bool> {
        if let Ok(config) = self.config.read() {
            Ok(config.ime_enabled.value)
        } else {
            Ok(false)
        }
    }

    pub fn set_enabled(&self, on_off: bool) -> Result<()> {
        if self.on_off_state_locked.get()? {
            return Ok(());
        }

        if let Ok(mut config) = self.config.write() {
            if config.ime_enabled.value != on_off {
                let mut enabled = BoolValue::new();
                enabled.value = on_off;
                config.ime_enabled = MessageField::some(enabled);
            }
        }

        if !on_off {
            // TODO: commit outstanding buffer
        }

        Ok(())
    }

    pub fn toggle_enabled(&self) -> Result<()> {
        Ok(())
    }

    pub fn engine(&self) -> Arc<RwLock<EngineMgr>> {
        self.engine.clone()
    }

    pub fn set_this(&self, this: ITfTextInputProcessor) {
        self.this.replace(Some(this));
    }

    pub fn this(&self) -> ITfTextInputProcessor {
        self.this.borrow().clone().unwrap()
    }

    pub fn threadmgr(&self) -> ITfThreadMgr {
        self.threadmgr.borrow().clone().unwrap()
    }

    pub fn categorymgr(&self) -> Result<ITfCategoryMgr> {
        co_create_inproc(&CLSID_TF_CategoryMgr)
    }

    pub fn ui_element(&self) -> Result<ITfUIElement> {
        self.candidate_list_ui.borrow().clone().unwrap().cast()
    }

    pub fn notify_command(
        &self,
        ec: u32,
        context: ITfContext,
        command: Arc<Command>,
    ) -> Result<()> {
        if let Ok(mut mgr) = self.composition_mgr.write() {
            let sink: ITfCompositionSink = self.this().cast()?;
            return mgr.notify_command(ec, context, sink, command);
        }

        Ok(())
    }
}

// Private portion
impl TextService {
    fn activate(&self) -> Result<()> {
        DllModule::global().add_ref();
        PopupMenu::register_class(DllModule::global().module);
        self.init_engine()?;
        self.init_lang_bar_indicator()?;
        self.init_threadmgr_event_sink()?;
        self.init_candidate_ui()?;
        self.init_open_close_compartment()?;
        self.init_config_compartment()?;
        self.init_userdata_compartment()?;
        self.init_kbd_disabled_compartment()?;
        self.init_preserved_key_mgr()?;
        self.init_key_event_sink()?;
        self.init_display_attributes()?;
        self.set_enabled(true)?;
        Ok(())
    }

    fn deactivate(&self) -> Result<()> {
        self.set_enabled(false).ok();
        self.deinit_display_attributes().ok();
        self.deinit_key_event_sink().ok();
        self.deinit_preserved_key_mgr().ok();
        self.deinit_kbd_disabled_compartment().ok();
        self.deinit_userdata_compartment().ok();
        self.deinit_config_compartment().ok();
        self.deinit_open_close_compartment().ok();
        self.deinit_candidate_ui().ok();
        self.deinit_threadmgr_event_sink().ok();
        self.deinit_lang_bar_indicator().ok();
        self.deinit_engine().ok();
        PopupMenu::unregister_class(DllModule::global().module);
        DllModule::global().release();
        Ok(())
    }

    fn init_engine(&self) -> Result<()> {
        if let Ok(mut engine) = self.engine.write() {
            engine.init(self.this());
        }
        Ok(())
    }

    fn deinit_engine(&self) -> Result<()> {
        if let Ok(mut engine) = self.engine.write() {
            engine.deinit();
        }
        Ok(())
    }

    // compartments & sinkmgrs
    fn init_compartment(
        &self,
        guid: GUID,
        compartment: &Arc<RwLock<Compartment>>,
        sinkmgr: &RefCell<SinkMgr<ITfCompartmentEventSink>>,
    ) -> Result<()> {
        if let Ok(mut comp) = compartment.write() {
            comp.init_thread(self.threadmgr(), self.clientid()?, guid);
            let mut sinkmgr = sinkmgr.borrow_mut();
            let punk: IUnknown = comp.compartment()?.cast()?;
            let this: ITfCompartmentEventSink = self.this().cast()?;
            sinkmgr.advise(punk, this)
        } else {
            winerr!(E_FAIL)
        }
    }

    fn deinit_compartment(
        &self,
        compartment: &Arc<RwLock<Compartment>>,
        sinkmgr: &RefCell<SinkMgr<ITfCompartmentEventSink>>,
    ) -> Result<()> {
        sinkmgr.borrow_mut().unadvise()?;
        match compartment.write() {
            Ok(mut comp) => comp.deinit(),
            Err(_) => winerr!(E_FAIL),
        }
    }

    fn get_compartment_bool(
        &self,
        compartment: &Arc<RwLock<Compartment>>,
    ) -> Result<bool> {
        match compartment.read() {
            Ok(comp) => comp.get_bool(),
            Err(_) => winerr!(E_FAIL),
        }
    }

    fn get_compartment_u32(
        &self,
        compartment: &Arc<RwLock<Compartment>>,
    ) -> Result<u32> {
        match compartment.read() {
            Ok(comp) => comp.get_value(),
            Err(_) => winerr!(E_FAIL),
        }
    }

    // open-close compartment
    fn init_open_close_compartment(&self) -> Result<()> {
        self.init_compartment(
            GUID_COMPARTMENT_KEYBOARD_OPENCLOSE,
            &self.open_close_compartment,
            &self.open_close_sinkmgr,
        )?;
        self.set_open_close_compartment(true)
    }

    fn deinit_open_close_compartment(&self) -> Result<()> {
        let _ = self.set_open_close_compartment(false);
        self.deinit_compartment(
            &self.open_close_compartment,
            &self.open_close_sinkmgr,
        )
    }

    fn set_open_close_compartment(&self, value: bool) -> Result<()> {
        match self.open_close_compartment.read() {
            Ok(comp) => comp.set_bool(value),
            Err(_) => winerr!(E_FAIL),
        }
    }

    // config compartment
    fn init_config_compartment(&self) -> Result<()> {
        self.init_compartment(
            GUID_CONFIG_CHANGED_COMPARTMENT,
            &self.config_compartment,
            &self.config_sinkmgr,
        )
    }

    fn deinit_config_compartment(&self) -> Result<()> {
        self.deinit_compartment(&self.config_compartment, &self.config_sinkmgr)
    }

    // userdata compartment
    fn init_userdata_compartment(&self) -> Result<()> {
        self.init_compartment(
            GUID_RESET_USERDATA_COMPARTMENT,
            &self.userdata_compartment,
            &self.userdata_sinkmgr,
        )
    }

    fn deinit_userdata_compartment(&self) -> Result<()> {
        self.deinit_compartment(
            &self.userdata_compartment,
            &self.userdata_sinkmgr,
        )
    }

    // keyboard disabled compartment
    fn init_kbd_disabled_compartment(&self) -> Result<()> {
        self.init_compartment(
            GUID_COMPARTMENT_KEYBOARD_DISABLED,
            &self.kbd_disabled_compartment,
            &self.kbd_disabled_sinkmgr,
        )
    }

    fn deinit_kbd_disabled_compartment(&self) -> Result<()> {
        self.deinit_compartment(
            &self.kbd_disabled_compartment,
            &self.kbd_disabled_sinkmgr,
        )
    }

    // key event sink
    fn init_key_event_sink(&self) -> Result<()> {
        let sink = KeyEventSink::new(self.this(), self.threadmgr());
        let sink: ITfKeyEventSink = sink.into();
        self.key_event_sink.replace(Some(sink));
        self.key_event_sink().as_impl().advise()
    }

    fn deinit_key_event_sink(&self) -> Result<()> {
        let _ = self.key_event_sink().as_impl().unadvise();
        self.key_event_sink.replace(None);
        Ok(())
    }

    fn key_event_sink(&self) -> ITfKeyEventSink {
        self.key_event_sink.borrow().clone().unwrap()
    }

    // threadmgr event sink
    fn init_threadmgr_event_sink(&self) -> Result<()> {
        let tip: ITfTextInputProcessor = self.this();
        self.threadmgr_event_sink
            .replace(Some(ThreadMgrEventSink::new(tip).into()));
        let sink = self.threadmgr_event_sink.borrow().clone().unwrap();
        let punk: IUnknown = self.threadmgr().cast()?;
        self.threadmgr_event_sink_sinkmgr
            .borrow_mut()
            .advise(punk, sink)
    }

    fn deinit_threadmgr_event_sink(&self) -> Result<()> {
        self.threadmgr_event_sink_sinkmgr.borrow_mut().unadvise()?;
        self.threadmgr_event_sink.replace(None);
        Ok(())
    }

    // preseved key manager
    fn init_preserved_key_mgr(&self) -> Result<()> {
        self.preserved_key_mgr
            .replace(Some(PreservedKeyMgr::new(self.this())));

        Ok(())
    }

    fn deinit_preserved_key_mgr(&self) -> Result<()> {
        Ok(())
    }

    // language bar indicator
    fn init_lang_bar_indicator(&self) -> Result<()> {
        let indicator = LangBarIndicator::new(self.this(), self.threadmgr())?;
        self.lang_bar_indicator.replace(Some(indicator));
        Ok(())
    }

    fn deinit_lang_bar_indicator(&self) -> Result<()> {
        let button = self.lang_bar_indicator().clone();
        let indicator = button.clone();
        let indicator = indicator.as_impl();
        let _ = indicator.shutdown(button);
        // logging?
        self.lang_bar_indicator.replace(None);
        Ok(())
    }

    fn lang_bar_indicator(&self) -> ITfLangBarItemButton {
        self.lang_bar_indicator.borrow().clone().unwrap()
    }

    // candidate ui
    fn init_candidate_ui(&self) -> Result<()> {
        self.candidate_list_ui
            .replace(Some(CandidateListUI::new(self.this()).into()));
        Ok(())
    }

    fn deinit_candidate_ui(&self) -> Result<()> {
        self.candidate_list_ui.replace(None);
        Ok(())
    }

    // display attributes (underlines)
    fn init_display_attributes(&self) -> Result<()> {
        let categorymgr = self.categorymgr()?;
        unsafe {
            self.input_attr_guidatom.set(
                categorymgr.RegisterGUID(&GUID_DISPLAY_ATTRIBUTE_INPUT)?,
            )?;
            self.converted_attr_guidatom.set(
                categorymgr.RegisterGUID(&GUID_DISPLAY_ATTRIBUTE_CONVERTED)?,
            )?;
            self.focused_attr_guidatom.set(
                categorymgr.RegisterGUID(&GUID_DISPLAY_ATTRIBUTE_FOCUSED)?,
            )
        }
    }

    fn deinit_display_attributes(&self) -> Result<()> {
        Ok(())
    }
}
//+---------------------------------------------------------------------------
//
// ITfCompartmentEventSink
//
//----------------------------------------------------------------------------

impl ITfCompartmentEventSink_Impl for TextService {
    fn OnChange(&self, rguid: *const GUID) -> Result<()> {
        let rguid = unsafe { *rguid };

        match rguid {
            GUID_COMPARTMENT_KEYBOARD_OPENCLOSE => Ok(()),
            GUID_CONFIG_CHANGED_COMPARTMENT => Ok(()),
            GUID_RESET_USERDATA_COMPARTMENT => Ok(()),
            GUID_COMPARTMENT_KEYBOARD_DISABLED => Ok(()),
            _ => Ok(()),
        }
    }
}

//+---------------------------------------------------------------------------
//
// ITfCompositionSink
//
//----------------------------------------------------------------------------

impl ITfCompositionSink_Impl for TextService {
    fn OnCompositionTerminated(
        &self,
        ecwrite: u32,
        pcomposition: Option<&ITfComposition>,
    ) -> Result<()> {
        Ok(())
    }
}

//+---------------------------------------------------------------------------
//
// ITfTextInputProcessor
//
//----------------------------------------------------------------------------

impl ITfTextInputProcessor_Impl for TextService {
    fn Activate(&self, ptim: Option<&ITfThreadMgr>, tid: u32) -> Result<()> {
        if self.ActivateEx(ptim, tid, 0).is_err() {
            self.deactivate()
        } else {
            Ok(())
        }
    }

    fn Deactivate(&self) -> Result<()> {
        self.deactivate()?;
        Ok(())
    }
}

//+---------------------------------------------------------------------------
//
// ITfTextInputProcessorEx
//
//----------------------------------------------------------------------------

impl ITfTextInputProcessorEx_Impl for TextService {
    fn ActivateEx(
        &self,
        ptim: Option<&ITfThreadMgr>,
        tid: u32,
        dwflags: u32,
    ) -> Result<()> {
        self.clientid.set(tid)?;
        self.dwflags.set(dwflags)?;

        match ptim {
            Some(threadmgr) => {
                let threadmgr = threadmgr.clone();
                self.threadmgr.replace(Some(threadmgr));
                self.activate()
            }
            None => Ok(()),
        }
    }
}
