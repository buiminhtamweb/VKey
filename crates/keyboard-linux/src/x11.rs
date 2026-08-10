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
                ConnectionExt as _, Device, EventMask, EventMode, GrabMode22, GrabOwner, GrabType,
                KeyPressEvent, RawKeyPressEvent, XIEventMask,
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
        #[allow(dead_code)]
        grab_modifiers: Vec<u32>,
        injection_keycode: u8,
        injection_keysyms_per_keycode: u8,
        original_injection_mapping: Vec<u32>,
        pending: Option<PendingEvent>,
        running: bool,
        xtest_device_ids: Vec<u16>,
        last_mapping_change: Option<std::time::Instant>,
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
            let mut xtest_device_ids = Vec::new();
            if let Ok(reply) = connection.xinput_xi_query_device(u16::from(Device::ALL)) {
                if let Ok(reply_data) = reply.reply() {
                    for info in reply_data.infos {
                        let name = String::from_utf8_lossy(&info.name).to_lowercase();
                        if name.contains("xtest") {
                            xtest_device_ids.push(info.deviceid);
                        }
                    }
                }
            }
            debug!(?xtest_device_ids, "Identified XTEST virtual devices");

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
                xtest_device_ids,
                last_mapping_change: None,
            })
        }

        #[allow(dead_code)]
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
                    warn!(
                        keycode,
                        modifier_mask = format_args!("{:#x}", failure.modifiers),
                        status = ?failure.status,
                        "skipping X11 key grab already owned by the window manager"
                    );
                    continue;
                }
                grabbed.push(keycode);
            }
            self.intercepted_keycodes = grabbed;
            self.connection
                .flush()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            Ok(())
        }

        #[allow(dead_code)]
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

        fn decode_raw(&mut self, event: &RawKeyPressEvent) -> Result<KeyEvent> {
            let keycode = u8::try_from(event.detail).map_err(|_| {
                KeyboardError::X11Protocol(format!(
                    "XInput2 returned invalid raw keycode {}",
                    event.detail
                ))
            })?;
            let xkb_keycode = xkb::Keycode::new(keycode.into());
            let keysym = self.state.key_get_one_sym(xkb_keycode);
            let decoded = KeyEvent::press(key_from_keysym(keysym.raw()));
            let decoded = KeyEvent {
                modifiers: self.current_modifiers(),
                ..decoded
            };
            self.state.update_key(xkb_keycode, xkb::KeyDirection::Down);
            debug!(?decoded, keycode, "observed X11 raw key press");
            Ok(decoded)
        }

        fn update_raw_release(&mut self, event: &RawKeyPressEvent) -> Result<()> {
            let keycode = u8::try_from(event.detail).map_err(|_| {
                KeyboardError::X11Protocol(format!(
                    "XInput2 returned invalid raw keycode {}",
                    event.detail
                ))
            })?;
            self.state
                .update_key(xkb::Keycode::new(keycode.into()), xkb::KeyDirection::Up);
            Ok(())
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

        fn current_modifiers(&self) -> Modifiers {
            let component = xkb::STATE_MODS_DEPRESSED
                | xkb::STATE_MODS_LATCHED
                | xkb::STATE_MODS_LOCKED
                | xkb::STATE_LAYOUT_DEPRESSED
                | xkb::STATE_LAYOUT_LATCHED
                | xkb::STATE_LAYOUT_LOCKED;
            Modifiers {
                shift: self
                    .state
                    .mod_name_is_active(xkb::MOD_NAME_SHIFT, component),
                ctrl: self.state.mod_name_is_active(xkb::MOD_NAME_CTRL, component),
                alt: self.state.mod_name_is_active(xkb::MOD_NAME_ALT, component),
                super_key: self.state.mod_name_is_active(xkb::MOD_NAME_LOGO, component),
                caps_lock: self.state.mod_name_is_active(xkb::MOD_NAME_CAPS, component),
                num_lock: self.state.mod_name_is_active(xkb::MOD_NAME_NUM, component),
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
            let injection_keycode = self.injection_keycode;

            // Check if any character in the text requires temporary mapping.
            let mut mapping_needed = false;
            for character in text.chars() {
                let keysym = Keysym::from_char(character).raw();
                if find_direct_keycode_in(&self.keymap, keysym).is_none() {
                    mapping_needed = true;
                    break;
                }
            }

            let mut mapping = TemporaryMapping::new(
                &self.connection,
                injection_keycode,
                self.injection_keysyms_per_keycode,
                &self.original_injection_mapping,
            );

            let injection_result = (|| {
                for character in text.chars() {
                    ensure_focus(mapping.connection(), expected)?;
                    let keysym = Keysym::from_char(character).raw();
                    let keycode = if !mapping_needed {
                        find_direct_keycode_in(&self.keymap, keysym).unwrap()
                    } else {
                        mapping.set_keysym(keysym)?;
                        injection_keycode
                    };
                    fake_key_on(mapping.connection(), keycode)?;
                    let delay = if keycode == injection_keycode { 40 } else { 15 };
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                }
                // Flush to ensure the key events reach the X server before
                // we restore the keymap.  This guarantees the application
                // decodes the injected keys using the temporary mapping.
                mapping
                    .connection()
                    .flush()
                    .map_err(|e| KeyboardError::ConnectionLost(e.to_string()))?;
                ensure_focus(mapping.connection(), expected)
            })();
            if mapping_needed {
                // Wait for the application to process the injected key events
                // using the temporary keymap BEFORE restoring the original
                // mapping.  Without this delay, the MappingNotify from
                // restore() may arrive before the application decodes the
                // injected keypress, causing it to use the restored (wrong)
                // mapping and lose the character.
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let restore_result = mapping.restore();
            if mapping_needed {
                self.last_mapping_change = Some(std::time::Instant::now());
            }
            injection_result.and(restore_result)
        }

        pub(crate) fn delete_graphemes(&mut self, count: usize) -> Result<()> {
            if count == 0 {
                return Ok(());
            }
            let expected = self.require_focused_window()?;

            if let Some(backspace_keycode) = find_direct_keycode_in(&self.keymap, key::BackSpace) {
                for _ in 0..count {
                    ensure_focus(&self.connection, expected)?;
                    fake_key_on(&self.connection, backspace_keycode)?;
                    std::thread::sleep(std::time::Duration::from_millis(15));
                }
                return Ok(());
            }

            let injection_keycode = self.injection_keycode;
            let mut mapping = TemporaryMapping::new(
                &self.connection,
                injection_keycode,
                self.injection_keysyms_per_keycode,
                &self.original_injection_mapping,
            );

            let injection_result = (|| {
                ensure_focus(mapping.connection(), expected)?;
                mapping.set_keysym(key::BackSpace)?;
                for _ in 0..count {
                    ensure_focus(mapping.connection(), expected)?;
                    fake_key_on(mapping.connection(), injection_keycode)?;
                    std::thread::sleep(std::time::Duration::from_millis(15));
                }
                ensure_focus(mapping.connection(), expected)
            })();
            let restore_result = mapping.restore();
            self.last_mapping_change = Some(std::time::Instant::now());
            injection_result.and(restore_result)
        }

        #[allow(dead_code)]
        fn find_direct_keycode(&self, target_keysym: u32) -> Option<u8> {
            find_direct_keycode_in(&self.keymap, target_keysym)
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
            let masks = [EventMask {
                deviceid: Device::ALL_MASTER.into(),
                mask: vec![XIEventMask::RAW_KEY_PRESS | XIEventMask::RAW_KEY_RELEASE],
            }];
            self.connection
                .xinput_xi_select_events(self.root, &masks)
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
                .check()
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            self.connection
                .flush()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            self.running = true;
            info!(
                keycodes = self.intercepted_keycodes.len(),
                "X11 keyboard observation started"
            );
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            if !self.running {
                return Ok(());
            }
            let masks = [EventMask {
                deviceid: Device::ALL_MASTER.into(),
                mask: Vec::new(),
            }];
            self.connection
                .xinput_xi_select_events(self.root, &masks)
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
                .check()
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            self.connection
                .flush()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            self.running = false;
            info!("X11 keyboard observation stopped");
            Ok(())
        }

        fn next_event(&mut self) -> Result<KeyEvent> {
            if !self.running {
                return Err(KeyboardError::NotRunning);
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
                    Event::XinputRawKeyPress(event) => {
                        // Skip events that were injected by XTEST.
                        if self.xtest_device_ids.contains(&event.deviceid)
                            || self.xtest_device_ids.contains(&event.sourceid)
                        {
                            continue;
                        }
                        if event.detail == u32::from(self.injection_keycode) {
                            continue;
                        }
                        return self.decode_raw(&event);
                    }
                    Event::XinputRawKeyRelease(event) => {
                        // Skip events that were injected by XTEST.
                        if self.xtest_device_ids.contains(&event.deviceid)
                            || self.xtest_device_ids.contains(&event.sourceid)
                        {
                            continue;
                        }
                        if event.detail == u32::from(self.injection_keycode) {
                            continue;
                        }
                        if let Err(error) = self.update_raw_release(&event) {
                            let _ = error;
                        }
                    }
                    Event::XinputKeyPress(event) => {
                        if self.xtest_device_ids.contains(&event.deviceid)
                            || self.xtest_device_ids.contains(&event.sourceid)
                        {
                            continue;
                        }
                        if event.detail == u32::from(self.injection_keycode) {
                            continue;
                        }
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
                        if self.xtest_device_ids.contains(&event.deviceid)
                            || self.xtest_device_ids.contains(&event.sourceid)
                        {
                            continue;
                        }
                        if event.detail == u32::from(self.injection_keycode) {
                            continue;
                        }
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
            if self.pending.is_none() {
                let _ = decision;
                return Ok(());
            }
            let pending = self
                .pending
                .take()
                .ok_or(KeyboardError::NoPendingDecision)?;

            // If a keymap change happened recently, delay thawing the device to let
            // the application reload the keymap and process the queue.
            if let Some(last_change) = self.last_mapping_change {
                let elapsed = last_change.elapsed();
                let threshold = std::time::Duration::from_millis(200);
                if elapsed < threshold {
                    std::thread::sleep(threshold - elapsed);
                }
            }

            let mode = EventMode::ASYNC_DEVICE;
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
            std::thread::sleep(std::time::Duration::from_millis(25));
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
            std::thread::sleep(std::time::Duration::from_millis(25));
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
                    .any(|keysym| key_is_interceptable(keysym.raw()))
            })
        })
    }

    fn key_is_interceptable(raw_keysym: u32) -> bool {
        let keysym = Keysym::new(raw_keysym);
        if keysym.is_modifier_key() {
            return is_supported_modifier_key(raw_keysym);
        }
        !matches!(key_from_keysym(raw_keysym), Key::Unknown | Key::F(_))
    }

    fn is_supported_modifier_key(raw_keysym: u32) -> bool {
        matches!(
            raw_keysym,
            key::Shift_L | key::Shift_R | key::Control_L | key::Control_R | key::Alt_L | key::Alt_R
        )
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

    /// Find the physical keycode for a keysym that is already mapped at layout 0, level 0.
    fn find_direct_keycode_in(keymap: &xkb::Keymap, target_keysym: u32) -> Option<u8> {
        let min = keymap.min_keycode().raw();
        let max = keymap.max_keycode().raw();
        for raw_keycode in min..=max {
            let xkb_keycode = xkb::Keycode::new(raw_keycode);
            if keymap.num_layouts_for_key(xkb_keycode) > 0
                && keymap.num_levels_for_key(xkb_keycode, 0) > 0
            {
                let syms = keymap.key_get_syms_by_level(xkb_keycode, 0, 0);
                if syms.iter().any(|&sym| sym.raw() == target_keysym) {
                    return u8::try_from(raw_keycode).ok();
                }
            }
        }
        None
    }

    fn fake_key_on(connection: &XCBConnection, keycode: u8) -> Result<()> {
        let send = |event_type| -> Result<()> {
            connection
                .xtest_fake_input(event_type, keycode, CURRENT_TIME, NONE, 0, 0, 0)
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
                .check()
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))
        };
        send(xproto::KEY_PRESS_EVENT)?;
        std::thread::sleep(std::time::Duration::from_millis(3));
        send(xproto::KEY_RELEASE_EVENT)
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
                "VKey-rs Phase 3 requires X11; XDG_SESSION_TYPE is wayland".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
pub use platform::X11KeyboardBackend;

#[cfg(target_os = "windows")]
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "windows")]
struct HookChannels {
    event_tx: std::sync::mpsc::Sender<vietnamese_core::KeyEvent>,
    decision_rx: std::sync::mpsc::Receiver<KeyboardDecision>,
}

#[cfg(target_os = "windows")]
static HOOK_CHANNELS: OnceLock<Mutex<Option<HookChannels>>> = OnceLock::new();

#[cfg(target_os = "windows")]
static HHOOK_HANDLE: OnceLock<Mutex<isize>> = OnceLock::new();

#[cfg(target_os = "windows")]
pub struct X11KeyboardBackend {
    running: bool,
    event_rx: std::sync::mpsc::Receiver<vietnamese_core::KeyEvent>,
    decision_tx: std::sync::mpsc::Sender<KeyboardDecision>,
    hook_thread_id: u32,
}

#[cfg(target_os = "windows")]
impl std::fmt::Debug for X11KeyboardBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("X11KeyboardBackend")
            .field("running", &self.running)
            .finish()
    }
}

#[cfg(target_os = "windows")]
impl X11KeyboardBackend {
    pub fn new() -> Result<Self> {
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        let (decision_tx, decision_rx) = std::sync::mpsc::channel();

        *HOOK_CHANNELS
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap() = Some(HookChannels {
            event_tx,
            decision_rx,
        });

        Ok(Self {
            running: false,
            event_rx,
            decision_tx,
            hook_thread_id: 0,
        })
    }

    pub(crate) fn focused_window(&self) -> Result<Option<WindowId>> {
        Ok(Some(WindowId(42)))
    }

    pub(crate) fn inject_text(&mut self, text: &str) -> Result<()> {
        unsafe { inject_unicode_text(text) }
    }

    pub(crate) fn delete_graphemes(&mut self, count: usize) -> Result<()> {
        unsafe { inject_backspaces(count) }
    }
}

#[cfg(target_os = "windows")]
impl KeyboardBackend for X11KeyboardBackend {
    fn start(&mut self) -> Result<()> {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetMessageW, MSG, SetWindowsHookExW, WH_KEYBOARD_LL,
        };

        if self.running {
            return Ok(());
        }

        self.running = true;

        let (thread_id_tx, thread_id_rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            unsafe {
                let thread_id = windows_sys::Win32::System::Threading::GetCurrentThreadId();
                let _ = thread_id_tx.send(thread_id);

                let hinstance = GetModuleHandleW(std::ptr::null());
                let hhook =
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), hinstance, 0);

                if !hhook.is_null() {
                    *HHOOK_HANDLE.get_or_init(|| Mutex::new(0)).lock().unwrap() = hhook as isize;

                    let mut msg: MSG = std::mem::zeroed();
                    while GetMessageW(&mut msg, std::ptr::null_mut() as _, 0, 0) > 0 {
                        // Pump messages
                    }
                }
            }
        });

        if let Ok(tid) = thread_id_rx.recv() {
            self.hook_thread_id = tid;
        }

        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            PostThreadMessageW, UnhookWindowsHookEx, WM_QUIT,
        };

        if !self.running {
            return Ok(());
        }

        self.running = false;

        // Clear HOOK_CHANNELS to drop the channel senders
        *HOOK_CHANNELS
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap() = None;

        unsafe {
            let mut hhook_guard = HHOOK_HANDLE.get_or_init(|| Mutex::new(0)).lock().unwrap();
            if *hhook_guard != 0 {
                UnhookWindowsHookEx(*hhook_guard as _);
                *hhook_guard = 0;
            }

            if self.hook_thread_id != 0 {
                PostThreadMessageW(self.hook_thread_id, WM_QUIT, 0, 0);
            }
        }

        Ok(())
    }

    fn next_event(&mut self) -> Result<vietnamese_core::KeyEvent> {
        if !self.running {
            return Err(crate::backend::KeyboardError::NotRunning);
        }

        self.event_rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .map_err(|e| match e {
                std::sync::mpsc::RecvTimeoutError::Timeout => {
                    crate::backend::KeyboardError::Timeout
                }
                std::sync::mpsc::RecvTimeoutError::Disconnected => {
                    crate::backend::KeyboardError::ConnectionLost(e.to_string())
                }
            })
    }

    fn decide(&mut self, decision: KeyboardDecision) -> Result<()> {
        let _ = self.decision_tx.send(decision);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running
    }
}

#[cfg(target_os = "windows")]
impl Drop for X11KeyboardBackend {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    if code >= 0 {
        let hook_struct = unsafe { *(lparam as *const KBDLLHOOKSTRUCT) };

        // Skip VKey-injected keys (LLKHF_INJECTED = 0x10) to prevent infinite loops
        if (hook_struct.flags & 0x10) == 0 {
            let is_key_down = wparam == WM_KEYDOWN as usize || wparam == WM_SYSKEYDOWN as usize;
            let is_key_up = wparam == WM_KEYUP as usize || wparam == WM_SYSKEYUP as usize;

            if is_key_down || is_key_up {
                let state = if is_key_down {
                    vietnamese_core::KeyState::Press
                } else {
                    vietnamese_core::KeyState::Release
                };

                let is_extended = (hook_struct.flags & 1) != 0;
                let key = map_vk_to_key(hook_struct.vkCode, hook_struct.scanCode, is_extended);
                let modifiers = unsafe { get_modifiers() };

                let event = vietnamese_core::KeyEvent {
                    key,
                    modifiers,
                    state,
                };

                let guard = HOOK_CHANNELS
                    .get_or_init(|| Mutex::new(None))
                    .lock()
                    .unwrap();
                if let Some(channels) = guard.as_ref() {
                    if channels.event_tx.send(event).is_ok() {
                        // Allow 50ms for daemon thread processing
                        if let Ok(decision) = channels
                            .decision_rx
                            .recv_timeout(std::time::Duration::from_millis(50))
                        {
                            if decision == KeyboardDecision::Consume {
                                return 1; // Consume key event
                            }
                        }
                    }
                }
            }
        }
    }

    let hhook = *HHOOK_HANDLE.get_or_init(|| Mutex::new(0)).lock().unwrap() as _;
    unsafe { CallNextHookEx(hhook, code, wparam, lparam) }
}

#[cfg(target_os = "windows")]
fn map_vk_to_key(vk: u32, scan_code: u32, is_extended: bool) -> vietnamese_core::Key {
    use vietnamese_core::Key;
    match vk {
        0x08 => Key::Backspace,
        0x0D => Key::Enter,
        0x1B => Key::Escape,
        0x09 => Key::Tab,
        0x20 => Key::Space,
        0x2E => Key::Delete,
        0x25 => Key::Left,
        0x27 => Key::Right,
        0x26 => Key::Up,
        0x28 => Key::Down,
        0x24 => Key::Home,
        0x23 => Key::End,
        0x21 => Key::PageUp,
        0x22 => Key::PageDown,
        0x2D => Key::Insert,
        0x14 => Key::CapsLock,
        0x90 => Key::NumLock,
        0x91 => Key::Unknown, // Map ScrollLock to Unknown since vietnamese_core::Key doesn't have it
        0x10 | 0xA0 | 0xA1 => Key::Shift,
        0x11 | 0xA2 | 0xA3 => Key::Control,
        0x12 | 0xA4 | 0xA5 => Key::Alt,
        0x5B | 0x5C => Key::Super,
        0x70..=0x87 => Key::F((vk - 0x70 + 1) as u8),
        _ => unsafe {
            if let Some(c) = vk_to_char(vk, scan_code, is_extended) {
                Key::Character(c)
            } else {
                Key::Unknown
            }
        },
    }
}

#[cfg(target_os = "windows")]
unsafe fn vk_to_char(vk: u32, scan_code: u32, is_extended: bool) -> Option<char> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardState, ToUnicode};

    let mut keyboard_state = [0u8; 256];
    if unsafe { GetKeyboardState(keyboard_state.as_mut_ptr()) } == 0 {
        return None;
    }

    let mut buffer = [0u16; 8];
    let flags = if is_extended { 1 } else { 0 };
    let len = unsafe {
        ToUnicode(
            vk,
            scan_code,
            keyboard_state.as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len() as i32,
            flags,
        )
    };

    if len > 0 {
        let text = String::from_utf16_lossy(&buffer[..len as usize]);
        text.chars().next()
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
unsafe fn get_modifiers() -> vietnamese_core::Modifiers {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, VK_CAPITAL, VK_CONTROL, VK_LWIN, VK_MENU, VK_NUMLOCK, VK_RWIN, VK_SHIFT,
    };

    let ctrl = unsafe { GetKeyState(VK_CONTROL as i32) < 0 };
    let shift = unsafe { GetKeyState(VK_SHIFT as i32) < 0 };
    let alt = unsafe { GetKeyState(VK_MENU as i32) < 0 };
    let super_key =
        unsafe { (GetKeyState(VK_LWIN as i32) < 0) || (GetKeyState(VK_RWIN as i32) < 0) };
    let caps_lock = unsafe { (GetKeyState(VK_CAPITAL as i32) & 1) != 0 };
    let num_lock = unsafe { (GetKeyState(VK_NUMLOCK as i32) & 1) != 0 };

    vietnamese_core::Modifiers {
        ctrl,
        shift,
        alt,
        super_key,
        caps_lock,
        num_lock,
    }
}

#[cfg(target_os = "windows")]
unsafe fn inject_unicode_text(text: &str) -> Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    };

    let mut inputs = Vec::new();
    for c in text.encode_utf16() {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: c,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: c,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }

    unsafe { send_input_batch(&inputs) }
}

#[cfg(target_os = "windows")]
unsafe fn inject_backspaces(count: usize) -> Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_BACK,
    };

    let mut inputs = Vec::new();
    for _ in 0..count {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_BACK,
                    wScan: 0,
                    dwFlags: 0,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_BACK,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }

    unsafe { send_input_batch(&inputs) }
}

#[cfg(target_os = "windows")]
unsafe fn send_input_batch(
    inputs: &[windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT],
) -> Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{INPUT, SendInput};

    if inputs.is_empty() {
        return Ok(());
    }

    let expected = u32::try_from(inputs.len()).map_err(|_| {
        KeyboardError::TextInjection("too many Windows INPUT records in one batch".to_owned())
    })?;
    let inserted = unsafe {
        SendInput(
            expected,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if inserted == expected {
        Ok(())
    } else {
        Err(KeyboardError::TextInjection(format!(
            "Windows SendInput accepted {inserted}/{expected} records ({})",
            std::io::Error::last_os_error()
        )))
    }
}

#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
pub struct X11KeyboardBackend {
    running: bool,
    queue: std::collections::VecDeque<vietnamese_core::KeyEvent>,
}

#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
impl std::fmt::Debug for X11KeyboardBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("X11KeyboardBackend")
            .field("running", &self.running)
            .finish()
    }
}

#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
impl X11KeyboardBackend {
    pub fn new() -> Result<Self> {
        println!("------------------------------------------------------------");
        println!("WARNING: Keyboard hook backend is not implemented on this OS.");
        println!("Running in Mock Stdin Keyboard mode for simulation.");
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

#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
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
            return Err(crate::backend::KeyboardError::NotRunning);
        }

        while self.queue.is_empty() {
            let mut input = String::new();
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            if handle.read_line(&mut input).is_err() {
                return Err(crate::backend::KeyboardError::ConnectionLost(
                    "failed to read from stdin".to_owned(),
                ));
            }

            let trimmed = input.trim_end_matches(['\r', '\n']);

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
                        "toggle" | "ctrlshift" => {
                            self.queue.push_back(KeyEvent {
                                key: Key::Control,
                                modifiers: vietnamese_core::Modifiers {
                                    ctrl: true,
                                    ..Default::default()
                                },
                                state: vietnamese_core::KeyState::Press,
                            });
                            self.queue.push_back(KeyEvent {
                                key: Key::Shift,
                                modifiers: vietnamese_core::Modifiers {
                                    ctrl: true,
                                    shift: true,
                                    ..Default::default()
                                },
                                state: vietnamese_core::KeyState::Press,
                            });
                            self.queue.push_back(KeyEvent {
                                key: Key::Shift,
                                modifiers: vietnamese_core::Modifiers {
                                    ctrl: true,
                                    ..Default::default()
                                },
                                state: vietnamese_core::KeyState::Release,
                            });
                            self.queue.push_back(KeyEvent {
                                key: Key::Control,
                                modifiers: Default::default(),
                                state: vietnamese_core::KeyState::Release,
                            });
                        }
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

            if !trimmed.is_empty() && trimmed != "escape" && trimmed != "esc" && trimmed != "exit" {
                self.queue.push_back(KeyEvent::press(Key::Enter));
            }
        }

        Ok(self
            .queue
            .pop_front()
            .unwrap_or_else(|| KeyEvent::press(Key::Unknown)))
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
