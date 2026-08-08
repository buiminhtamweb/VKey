use vietnamese_core::Key;
use xkeysym::{Keysym, key};

#[cfg(not(target_os = "linux"))]
use crate::{KeyboardBackend, KeyboardDecision, KeyboardError, Result, WindowId};

#[allow(dead_code)]
fn key_from_keysym(raw_keysym: u32) -> Key {
    #[allow(non_upper_case_globals)]
    match raw_keysym {
        key::BackSpace => Key::Backspace,
        key::Return | key::KP_Enter => Key::Enter,
        key::Escape => Key::Escape,
        key::Tab | key::ISO_Left_Tab | key::KP_Tab => Key::Tab,
        key::space | key::KP_Space => Key::Space,
        key::Delete | key::KP_Delete => Key::Delete,
        key::Left | key::KP_Left => Key::Left,
        key::Right | key::KP_Right => Key::Right,
        key::Up | key::KP_Up => Key::Up,
        key::Down | key::KP_Down => Key::Down,
        key::Home | key::KP_Home => Key::Home,
        key::End | key::KP_End => Key::End,
        key::Page_Up | key::KP_Page_Up => Key::PageUp,
        key::Page_Down | key::KP_Page_Down => Key::PageDown,
        key::Insert | key::KP_Insert => Key::Insert,
        key::Shift_L | key::Shift_R => Key::Shift,
        key::Control_L | key::Control_R => Key::Control,
        key::Alt_L | key::Alt_R | key::Meta_L | key::Meta_R => Key::Alt,
        key::Super_L | key::Super_R | key::Hyper_L | key::Hyper_R => Key::Super,
        key::Caps_Lock => Key::CapsLock,
        key::Num_Lock => Key::NumLock,
        key::F1..=key::F12 => Key::F((raw_keysym - key::F1 + 1) as u8),
        _ => Keysym::new(raw_keysym)
            .key_char()
            .map_or(Key::Unknown, Key::Character),
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{env, mem};

    use tracing::{debug, info, trace, warn};
    use vietnamese_core::{Key, KeyEvent, Modifiers};
    use x11rb::{
        CURRENT_TIME, NONE,
        connection::Connection,
        protocol::{
            Event,
            xinput::{
                ConnectionExt as _, Device, EventMode, GrabMode22, GrabOwner, GrabType,
                KeyPressEvent, XIEventMask,
            },
            xproto::{self, ConnectionExt as _, GrabMode, GrabStatus},
            xtest::ConnectionExt as _,
        },
        xcb_ffi::XCBConnection,
    };
    use xkbcommon::xkb;
    use xkeysym::{Keysym, key};

    use super::key_from_keysym;
    use crate::{KeyboardBackend, KeyboardDecision, KeyboardError, Result, WindowId};

    const REQUESTED_XI_MAJOR: u16 = 2;
    const REQUESTED_XI_MINOR: u16 = 2;
    const REQUESTED_XTEST_MAJOR: u8 = 2;
    const REQUESTED_XTEST_MINOR: u16 = 1;

    #[derive(Debug, Clone, Copy)]
    struct PendingEvent {
        device_id: u16,
        time: u32,
    }

    pub struct X11KeyboardBackend {
        connection: XCBConnection,
        root: u32,
        state: xkb::State,
        keymap: xkb::Keymap,
        _context: xkb::Context,
        intercepted_keycodes: Vec<u8>,
        grab_modifiers: Vec<u32>,
        injection_keycode: u8,
        injection_keysyms_per_keycode: u8,
        original_injection_mapping: Vec<u32>,
        pending: Option<PendingEvent>,
        running: bool,
    }

    impl std::fmt::Debug for X11KeyboardBackend {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("X11KeyboardBackend")
                .field("root", &self.root)
                .field("intercepted_keycodes", &self.intercepted_keycodes.len())
                .field("injection_keycode", &self.injection_keycode)
                .field("has_pending_decision", &self.pending.is_some())
                .field("running", &self.running)
                .finish_non_exhaustive()
        }
    }

    impl X11KeyboardBackend {
        pub fn new() -> Result<Self> {
            ensure_display_is_set()?;
            ensure_x11_session()?;
            let (connection, screen_number) = XCBConnection::connect(None)
                .map_err(|error| KeyboardError::X11Connection(error.to_string()))?;
            let root = connection
                .setup()
                .roots
                .get(screen_number)
                .ok_or_else(|| {
                    KeyboardError::X11Protocol(format!(
                        "X11 returned invalid screen index {screen_number}"
                    ))
                })?
                .root;

            let xi_version = connection
                .xinput_xi_query_version(REQUESTED_XI_MAJOR, REQUESTED_XI_MINOR)
                .map_err(|error| KeyboardError::XInputUnavailable(error.to_string()))?
                .reply()
                .map_err(|error| KeyboardError::XInputUnavailable(error.to_string()))?;
            if xi_version.major_version < 2 {
                return Err(KeyboardError::XInputUnavailable(format!(
                    "server supports {}.{}, but XInput 2.0 or newer is required",
                    xi_version.major_version, xi_version.minor_version
                )));
            }

            let xtest_version = connection
                .xtest_get_version(REQUESTED_XTEST_MAJOR, REQUESTED_XTEST_MINOR)
                .map_err(|error| KeyboardError::XTestUnavailable(error.to_string()))?
                .reply()
                .map_err(|error| KeyboardError::XTestUnavailable(error.to_string()))?;
            if xtest_version.major_version < 2 {
                return Err(KeyboardError::XTestUnavailable(format!(
                    "server supports {}.{}, but XTEST 2.1 is required",
                    xtest_version.major_version, xtest_version.minor_version
                )));
            }

            let mut xkb_major = 0;
            let mut xkb_minor = 0;
            let mut xkb_event_base = 0;
            let mut xkb_error_base = 0;
            if !xkb::x11::setup_xkb_extension(
                &connection,
                xkb::x11::MIN_MAJOR_XKB_VERSION,
                xkb::x11::MIN_MINOR_XKB_VERSION,
                xkb::x11::SetupXkbExtensionFlags::NoFlags,
                &mut xkb_major,
                &mut xkb_minor,
                &mut xkb_event_base,
                &mut xkb_error_base,
            ) {
                return Err(KeyboardError::XkbUnavailable(
                    "failed to initialize the XKB X11 extension".to_owned(),
                ));
            }

            let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
            if context.get_raw_ptr().is_null() {
                mem::forget(context);
                return Err(KeyboardError::XkbUnavailable(
                    "failed to create an XKB context".to_owned(),
                ));
            }
            let device_id = xkb::x11::get_core_keyboard_device_id(&connection);
            if device_id < 0 {
                return Err(KeyboardError::XkbUnavailable(
                    "X server did not provide a core keyboard device".to_owned(),
                ));
            }
            let keymap = xkb::x11::keymap_new_from_device(
                &context,
                &connection,
                device_id,
                xkb::KEYMAP_COMPILE_NO_FLAGS,
            );
            if keymap.get_raw_ptr().is_null() {
                mem::forget(keymap);
                return Err(KeyboardError::XkbUnavailable(
                    "failed to load the active X11 keymap".to_owned(),
                ));
            }
            let state = xkb::x11::state_new_from_device(&keymap, &connection, device_id);
            if state.get_raw_ptr().is_null() {
                mem::forget(state);
                return Err(KeyboardError::XkbUnavailable(
                    "failed to load the active X11 keyboard state".to_owned(),
                ));
            }

            let (injection_keycode, injection_keysyms_per_keycode, original_mapping) =
                find_spare_keycode(&connection)?;
            let intercepted_keycodes = interceptable_keycodes(&keymap, injection_keycode);
            let grab_modifiers = safe_grab_modifiers(&keymap);

            info!(
                xi_major = xi_version.major_version,
                xi_minor = xi_version.minor_version,
                xtest_major = xtest_version.major_version,
                xtest_minor = xtest_version.minor_version,
                xkb_major,
                xkb_minor,
                injection_keycode,
                "X11 input controller connected"
            );
            Ok(Self {
                connection,
                root,
                state,
                keymap,
                _context: context,
                intercepted_keycodes,
                grab_modifiers,
                injection_keycode,
                injection_keysyms_per_keycode,
                original_injection_mapping: original_mapping,
                pending: None,
                running: false,
            })
        }

        fn grab_keys(&mut self) -> Result<()> {
            let mask = vec![u32::from(XIEventMask::KEY_PRESS | XIEventMask::KEY_RELEASE)];
            let mut grabbed = Vec::new();

            for &keycode in &self.intercepted_keycodes {
                let reply = self
                    .connection
                    .xinput_xi_passive_grab_device(
                        CURRENT_TIME,
                        self.root,
                        NONE,
                        u32::from(keycode),
                        Device::ALL_MASTER,
                        GrabType::KEYCODE,
                        GrabMode22::SYNC,
                        GrabMode::ASYNC,
                        GrabOwner::NO_OWNER,
                        &mask,
                        &self.grab_modifiers,
                    )
                    .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
                    .reply()
                    .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;

                if let Some(failure) = reply
                    .modifiers
                    .iter()
                    .find(|item| item.status != GrabStatus::SUCCESS)
                {
                    self.ungrab_keycodes(&grabbed);
                    return Err(KeyboardError::X11Protocol(format!(
                        "cannot grab keycode {keycode} with modifier mask {:#x}: {:?}",
                        failure.modifiers, failure.status
                    )));
                }
                grabbed.push(keycode);
            }
            self.connection
                .flush()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            Ok(())
        }

        fn ungrab_keycodes(&self, keycodes: &[u8]) {
            for &keycode in keycodes {
                match self.connection.xinput_xi_passive_ungrab_device(
                    self.root,
                    u32::from(keycode),
                    Device::ALL_MASTER,
                    GrabType::KEYCODE,
                    &self.grab_modifiers,
                ) {
                    Ok(cookie) => {
                        if let Err(error) = cookie.check() {
                            warn!(keycode, %error, "failed to remove passive X11 key grab");
                        }
                    }
                    Err(error) => {
                        warn!(keycode, %error, "failed to send passive X11 ungrab request");
                    }
                }
            }
        }

        fn decode(&mut self, event: &KeyPressEvent) -> Result<KeyEvent> {
            let keycode = u8::try_from(event.detail).map_err(|_| {
                KeyboardError::X11Protocol(format!(
                    "XInput2 returned invalid keycode {}",
                    event.detail
                ))
            })?;
            self.state.update_mask(
                event.mods.base,
                event.mods.latched,
                event.mods.locked,
                u32::from(event.group.base),
                u32::from(event.group.latched),
                u32::from(event.group.locked),
            );
            let keysym = self
                .state
                .key_get_one_sym(xkb::Keycode::new(keycode.into()));
            let decoded = KeyEvent::press(key_from_keysym(keysym.raw()));
            let decoded = KeyEvent {
                modifiers: self.modifiers(event.mods.effective),
                ..decoded
            };
            debug!(?decoded, keycode, "captured X11 key event");
            Ok(decoded)
        }

        fn modifiers(&self, effective: u32) -> Modifiers {
            Modifiers {
                shift: modifier_is_active(&self.keymap, xkb::MOD_NAME_SHIFT, effective),
                ctrl: modifier_is_active(&self.keymap, xkb::MOD_NAME_CTRL, effective),
                alt: modifier_is_active(&self.keymap, xkb::MOD_NAME_ALT, effective),
                super_key: modifier_is_active(&self.keymap, xkb::MOD_NAME_LOGO, effective),
                caps_lock: modifier_is_active(&self.keymap, xkb::MOD_NAME_CAPS, effective),
                num_lock: modifier_is_active(&self.keymap, xkb::MOD_NAME_NUM, effective),
            }
        }

        pub(crate) fn focused_window(&self) -> Result<Option<WindowId>> {
            focused_window(&self.connection)
        }

        pub(crate) fn inject_text(&mut self, text: &str) -> Result<()> {
            if text.is_empty() {
                return Ok(());
            }
            let expected = self.require_focused_window()?;
            let mut mapping = TemporaryMapping::new(
                &self.connection,
                self.injection_keycode,
                self.injection_keysyms_per_keycode,
                &self.original_injection_mapping,
            );

            let injection_result = (|| {
                for character in text.chars() {
                    ensure_focus(mapping.connection(), expected)?;
                    mapping.set_keysym(Keysym::from_char(character).raw())?;
                    fake_key(mapping.connection(), self.injection_keycode)?;
                }
                ensure_focus(mapping.connection(), expected)
            })();
            let restore_result = mapping.restore();
            injection_result.and(restore_result)
        }

        pub(crate) fn delete_graphemes(&mut self, count: usize) -> Result<()> {
            if count == 0 {
                return Ok(());
            }
            let expected = self.require_focused_window()?;
            let mut mapping = TemporaryMapping::new(
                &self.connection,
                self.injection_keycode,
                self.injection_keysyms_per_keycode,
                &self.original_injection_mapping,
            );

            let injection_result = (|| {
                ensure_focus(mapping.connection(), expected)?;
                mapping.set_keysym(key::BackSpace)?;
                for _ in 0..count {
                    ensure_focus(mapping.connection(), expected)?;
                    fake_key(mapping.connection(), self.injection_keycode)?;
                }
                ensure_focus(mapping.connection(), expected)
            })();
            let restore_result = mapping.restore();
            injection_result.and(restore_result)
        }

        fn require_focused_window(&self) -> Result<WindowId> {
            self.focused_window()?.ok_or(KeyboardError::NoFocusedWindow)
        }
    }

    impl KeyboardBackend for X11KeyboardBackend {
        fn start(&mut self) -> Result<()> {
            if self.running {
                return Ok(());
            }
            self.grab_keys()?;
            self.running = true;
            info!(
                keycodes = self.intercepted_keycodes.len(),
                "X11 keyboard interception started"
            );
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            if !self.running {
                return Ok(());
            }
            if let Some(pending) = self.pending.take() {
                if let Ok(cookie) = self.connection.xinput_xi_allow_events(
                    pending.time,
                    pending.device_id,
                    EventMode::ASYNC_DEVICE,
                    0,
                    self.root,
                ) {
                    let _ = cookie.check();
                }
            }
            self.ungrab_keycodes(&self.intercepted_keycodes);
            self.connection
                .flush()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            self.running = false;
            info!("X11 keyboard interception stopped");
            Ok(())
        }

        fn next_event(&mut self) -> Result<KeyEvent> {
            if !self.running {
                return Err(KeyboardError::NotRunning);
            }
            if self.pending.is_some() {
                return Err(KeyboardError::PendingDecision);
            }

            loop {
                let event = match self.connection.wait_for_event() {
                    Ok(event) => event,
                    Err(error) => {
                        self.running = false;
                        return Err(KeyboardError::ConnectionLost(error.to_string()));
                    }
                };
                match event {
                    Event::XinputKeyPress(event) => {
                        self.pending = Some(PendingEvent {
                            device_id: event.deviceid,
                            time: event.time,
                        });
                        return match self.decode(&event) {
                            Ok(decoded) => Ok(decoded),
                            Err(error) => {
                                let _ = self.decide(KeyboardDecision::PassThrough);
                                Err(error)
                            }
                        };
                    }
                    Event::XinputKeyRelease(event) => {
                        trace!(
                            keycode = event.detail,
                            "ignored release from active X11 grab"
                        );
                    }
                    other => trace!(event = ?other, "ignored non-keyboard X11 event"),
                }
            }
        }

        fn decide(&mut self, decision: KeyboardDecision) -> Result<()> {
            if !self.running {
                return Err(KeyboardError::NotRunning);
            }
            let pending = self
                .pending
                .take()
                .ok_or(KeyboardError::NoPendingDecision)?;
            let mode = match decision {
                KeyboardDecision::PassThrough => EventMode::REPLAY_DEVICE,
                KeyboardDecision::Consume => EventMode::ASYNC_DEVICE,
            };
            let allow_result = self
                .connection
                .xinput_xi_allow_events(pending.time, pending.device_id, mode, 0, self.root)
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))
                .and_then(|cookie| {
                    cookie
                        .check()
                        .map_err(|error| KeyboardError::X11Protocol(error.to_string()))
                });
            if let Err(error) = allow_result {
                if let Ok(cookie) = self.connection.xinput_xi_allow_events(
                    CURRENT_TIME,
                    pending.device_id,
                    EventMode::ASYNC_DEVICE,
                    0,
                    self.root,
                ) {
                    let _ = cookie.check();
                }
                return Err(error);
            }
            self.connection
                .flush()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            Ok(())
        }

        fn is_running(&self) -> bool {
            self.running
        }
    }

    impl Drop for X11KeyboardBackend {
        fn drop(&mut self) {
            if self.running {
                let _ = self.stop();
            }
        }
    }

    struct TemporaryMapping<'a> {
        connection: &'a XCBConnection,
        keycode: u8,
        keysyms_per_keycode: u8,
        original: &'a [u32],
        dirty: bool,
    }

    impl<'a> TemporaryMapping<'a> {
        fn new(
            connection: &'a XCBConnection,
            keycode: u8,
            keysyms_per_keycode: u8,
            original: &'a [u32],
        ) -> Self {
            Self {
                connection,
                keycode,
                keysyms_per_keycode,
                original,
                dirty: false,
            }
        }

        fn connection(&self) -> &XCBConnection {
            self.connection
        }

        fn set_keysym(&mut self, keysym: u32) -> Result<()> {
            let keysyms = vec![keysym; usize::from(self.keysyms_per_keycode)];
            let cookie = self
                .connection
                .change_keyboard_mapping(1, self.keycode, self.keysyms_per_keycode, &keysyms)
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            self.dirty = true;
            cookie
                .check()
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            Ok(())
        }

        fn restore(&mut self) -> Result<()> {
            if !self.dirty {
                return Ok(());
            }
            self.connection
                .change_keyboard_mapping(1, self.keycode, self.keysyms_per_keycode, self.original)
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
                .check()
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            self.dirty = false;
            Ok(())
        }
    }

    impl Drop for TemporaryMapping<'_> {
        fn drop(&mut self) {
            if let Err(error) = self.restore() {
                warn!(%error, "failed to restore temporary X11 keyboard mapping");
            }
        }
    }

    fn find_spare_keycode(connection: &XCBConnection) -> Result<(u8, u8, Vec<u32>)> {
        let setup = connection.setup();
        let min = setup.min_keycode;
        let max = setup.max_keycode;
        let count = max
            .checked_sub(min)
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| KeyboardError::X11Protocol("invalid X11 keycode range".to_owned()))?;
        let reply = connection
            .get_keyboard_mapping(min, count)
            .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
            .reply()
            .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
        let width = usize::from(reply.keysyms_per_keycode);
        if width == 0 {
            return Err(KeyboardError::X11Protocol(
                "X11 returned a zero-width keyboard mapping".to_owned(),
            ));
        }
        for (offset, mapping) in reply.keysyms.chunks_exact(width).enumerate().rev() {
            if mapping.iter().all(|keysym| *keysym == 0) {
                let offset = u8::try_from(offset).map_err(|_| {
                    KeyboardError::X11Protocol("X11 keycode range is too large".to_owned())
                })?;
                let keycode = min
                    .checked_add(offset)
                    .ok_or_else(|| KeyboardError::X11Protocol("X11 keycode overflow".to_owned()))?;
                return Ok((keycode, reply.keysyms_per_keycode, mapping.to_vec()));
            }
        }
        Err(KeyboardError::NoSpareKeycode)
    }

    fn interceptable_keycodes(keymap: &xkb::Keymap, excluded: u8) -> Vec<u8> {
        let min = keymap.min_keycode().raw();
        let max = keymap.max_keycode().raw();
        (min..=max)
            .filter_map(|raw| u8::try_from(raw).ok())
            .filter(|keycode| *keycode != excluded)
            .filter(|keycode| keycode_is_interceptable(keymap, *keycode))
            .collect()
    }

    fn keycode_is_interceptable(keymap: &xkb::Keymap, keycode: u8) -> bool {
        let keycode = xkb::Keycode::new(keycode.into());
        (0..keymap.num_layouts_for_key(keycode)).any(|layout| {
            (0..keymap.num_levels_for_key(keycode, layout)).any(|level| {
                keymap
                    .key_get_syms_by_level(keycode, layout, level)
                    .iter()
                    .copied()
                    .any(|keysym| {
                        keysym.is_modifier_key()
                            || !matches!(key_from_keysym(keysym.raw()), Key::Unknown | Key::F(_))
                    })
            })
        })
    }

    fn safe_grab_modifiers(keymap: &xkb::Keymap) -> Vec<u32> {
        let mut masks = vec![0];
        for name in [xkb::MOD_NAME_SHIFT, xkb::MOD_NAME_CAPS, xkb::MOD_NAME_NUM] {
            let index = keymap.mod_get_index(name);
            if index == xkb::MOD_INVALID || index >= u32::BITS {
                continue;
            }
            let bit = 1_u32 << index;
            let existing = masks.clone();
            masks.extend(existing.into_iter().map(|mask| mask | bit));
        }
        masks.sort_unstable();
        masks.dedup();
        masks
    }

    fn modifier_is_active(keymap: &xkb::Keymap, name: &str, effective: u32) -> bool {
        let index = keymap.mod_get_index(name);
        index != xkb::MOD_INVALID && index < u32::BITS && effective & (1_u32 << index) != 0
    }

    fn fake_key(connection: &XCBConnection, keycode: u8) -> Result<()> {
        let send = |event_type| -> Result<()> {
            connection
                .xtest_fake_input(event_type, keycode, CURRENT_TIME, NONE, 0, 0, 0)
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
                .check()
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))
        };
        let press_result = send(xproto::KEY_PRESS_EVENT);
        // Always attempt the release, even if checking the press failed.
        let release_result = send(xproto::KEY_RELEASE_EVENT);
        press_result.and(release_result)
    }

    fn focused_window(connection: &XCBConnection) -> Result<Option<WindowId>> {
        let reply = connection
            .get_input_focus()
            .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
            .reply()
            .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
        if reply.focus == xproto::InputFocus::NONE.into()
            || reply.focus == xproto::InputFocus::POINTER_ROOT.into()
        {
            return Ok(None);
        }
        connection
            .get_window_attributes(reply.focus)
            .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
            .reply()
            .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
        Ok(Some(WindowId(reply.focus)))
    }

    fn ensure_focus(connection: &XCBConnection, expected: WindowId) -> Result<()> {
        let actual = focused_window(connection)?.map_or(0, |window| window.0);
        if actual == expected.0 {
            Ok(())
        } else {
            Err(KeyboardError::FocusChanged {
                expected: expected.0,
                actual,
            })
        }
    }

    fn ensure_display_is_set() -> Result<()> {
        match env::var_os("DISPLAY") {
            Some(display) if !display.is_empty() => Ok(()),
            _ => Err(KeyboardError::MissingDisplay),
        }
    }

    fn ensure_x11_session() -> Result<()> {
        if env::var("XDG_SESSION_TYPE").is_ok_and(|session| session.eq_ignore_ascii_case("wayland"))
        {
            return Err(KeyboardError::X11Connection(
                "openkey-rs Phase 3 requires X11; XDG_SESSION_TYPE is wayland".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub use platform::X11KeyboardBackend;

#[cfg(not(target_os = "linux"))]
pub struct X11KeyboardBackend {
    running: bool,
    queue: std::collections::VecDeque<vietnamese_core::KeyEvent>,
}

#[cfg(not(target_os = "linux"))]
impl std::fmt::Debug for X11KeyboardBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("X11KeyboardBackend")
            .field("running", &self.running)
            .finish()
    }
}

#[cfg(not(target_os = "linux"))]
impl X11KeyboardBackend {
    pub fn new() -> Result<Self> {
        println!("------------------------------------------------------------");
        println!("WARNING: X11 keyboard backend is only supported on Linux.");
        println!("Running in Mock Stdin Keyboard mode for Windows/macOS dev.");
        println!("Type characters and press Enter to simulate typing.");
        println!("Available escape sequences: \\b (backspace), \\esc (escape), \\enter, \\space, \\tab");
        println!("Type 'exit' or press Ctrl+C to terminate.");
        println!("------------------------------------------------------------");
        Ok(Self {
            running: false,
            queue: std::collections::VecDeque::new(),
        })
    }

    pub(crate) fn focused_window(&self) -> Result<Option<WindowId>> {
        Ok(Some(WindowId(42)))
    }

    pub(crate) fn inject_text(&mut self, text: &str) -> Result<()> {
        println!("  >> [MOCK INJECT] Text: {:?}", text);
        Ok(())
    }

    pub(crate) fn delete_graphemes(&mut self, count: usize) -> Result<()> {
        println!("  << [MOCK DELETE] Grapheme count: {}", count);
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
impl KeyboardBackend for X11KeyboardBackend {
    fn start(&mut self) -> Result<()> {
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    fn next_event(&mut self) -> Result<vietnamese_core::KeyEvent> {
        use std::io::{self, BufRead};
        use vietnamese_core::{Key, KeyEvent};

        if !self.running {
            return Err(KeyboardError::NotRunning);
        }

        while self.queue.is_empty() {
            let mut input = String::new();
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            if handle.read_line(&mut input).is_err() {
                return Err(KeyboardError::ConnectionLost("failed to read from stdin".to_owned()));
            }

            // Remove trailing newline
            let trimmed = input.trim_end_matches(['\r', '\n']);
            
            // Check for exit command
            if trimmed == "exit" {
                println!("Exiting mock mode...");
                std::process::exit(0);
            }

            let mut chars = trimmed.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    let mut seq = String::new();
                    while let Some(&next_c) = chars.peek() {
                        if next_c.is_alphabetic() {
                            seq.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    match seq.as_str() {
                        "backspace" | "b" => self.queue.push_back(KeyEvent::press(Key::Backspace)),
                        "escape" | "esc" => self.queue.push_back(KeyEvent::press(Key::Escape)),
                        "enter" => self.queue.push_back(KeyEvent::press(Key::Enter)),
                        "space" => self.queue.push_back(KeyEvent::press(Key::Space)),
                        "tab" => self.queue.push_back(KeyEvent::press(Key::Tab)),
                        "" => {
                            self.queue.push_back(KeyEvent::character('\\'));
                        }
                        _ => {
                            self.queue.push_back(KeyEvent::character('\\'));
                            for sc in seq.chars() {
                                self.queue.push_back(KeyEvent::character(sc));
                            }
                        }
                    }
                } else {
                    self.queue.push_back(KeyEvent::character(c));
                }
            }

            // Add an Enter press at the end of the line if the user typed something and we didn't end with Escape
            if !trimmed.is_empty() && trimmed != "escape" && trimmed != "esc" && trimmed != "exit" {
                self.queue.push_back(KeyEvent::press(Key::Enter));
            }
        }

        Ok(self.queue.pop_front().unwrap_or_else(|| KeyEvent::press(Key::Unknown)))
    }

    fn decide(&mut self, _decision: KeyboardDecision) -> Result<()> {
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_characters_without_assuming_keycodes() {
        assert_eq!(key_from_keysym(key::a), Key::Character('a'));
        assert_eq!(key_from_keysym(key::A), Key::Character('A'));
        assert_eq!(key_from_keysym(key::_1), Key::Character('1'));
        assert_eq!(key_from_keysym(key::exclam), Key::Character('!'));
        assert_eq!(key_from_keysym(key::eacute), Key::Character('é'));
    }

    #[test]
    fn maps_required_special_keys() {
        for (keysym, expected) in [
            (key::BackSpace, Key::Backspace),
            (key::Return, Key::Enter),
            (key::Escape, Key::Escape),
            (key::Tab, Key::Tab),
            (key::space, Key::Space),
            (key::Delete, Key::Delete),
            (key::Left, Key::Left),
            (key::Right, Key::Right),
            (key::Up, Key::Up),
            (key::Down, Key::Down),
            (key::Home, Key::Home),
            (key::End, Key::End),
            (key::Page_Up, Key::PageUp),
            (key::Page_Down, Key::PageDown),
            (key::Insert, Key::Insert),
        ] {
            assert_eq!(key_from_keysym(keysym), expected, "keysym: {keysym:#x}");
        }
    }

    #[test]
    fn maps_modifier_and_function_keys() {
        for (keysym, expected) in [
            (key::Shift_L, Key::Shift),
            (key::Shift_R, Key::Shift),
            (key::Control_L, Key::Control),
            (key::Control_R, Key::Control),
            (key::Alt_L, Key::Alt),
            (key::Alt_R, Key::Alt),
            (key::Super_L, Key::Super),
            (key::Super_R, Key::Super),
            (key::Caps_Lock, Key::CapsLock),
            (key::Num_Lock, Key::NumLock),
            (key::F1, Key::F(1)),
            (key::F12, Key::F(12)),
        ] {
            assert_eq!(key_from_keysym(keysym), expected, "keysym: {keysym:#x}");
        }
    }

    #[test]
    fn unknown_keysym_is_explicit() {
        assert_eq!(key_from_keysym(0), Key::Unknown);
    }
}
