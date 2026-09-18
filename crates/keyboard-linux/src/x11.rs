use vietnamese_core::Key;
use xkeysym::{Keysym, key};

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
        key::dead_belowdot => Key::Character('.'),
        key::dead_hook => Key::Character('?'),
        key::dead_acute => Key::Character('\''),
        key::dead_grave => Key::Character('`'),
        key::dead_tilde => Key::Character('~'),
        key::dead_circumflex => Key::Character('^'),
        _ => Keysym::new(raw_keysym)
            .key_char()
            .map_or(Key::Unknown, Key::Character),
    }
}

mod platform {
    use std::{
        collections::VecDeque,
        env, mem,
        os::fd::AsRawFd,
        time::{Duration, Instant},
    };

    use tracing::{debug, info, trace, warn};
    use vietnamese_core::{Key, KeyEvent, KeyState, Modifiers};
    use x11rb::{
        CURRENT_TIME, NONE,
        connection::Connection,
        protocol::{
            Event,
            xinput::{
                ConnectionExt as _, Device, DeviceType, EventMask, EventMode, GrabMode22,
                GrabOwner, GrabType, InputStateData, KeyPressEvent, RawKeyPressEvent, XIEventMask,
            },
            xkb::{ConnectionExt as _, ID as XkbDeviceId},
            xproto::{self, ConnectionExt as _, GrabMode, GrabStatus},
            xtest::ConnectionExt as _,
        },
        wrapper::ConnectionExt as _,
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
    const EVENT_WAIT_TIMEOUT: Duration = Duration::from_millis(50);

    #[derive(Debug, Clone, Copy)]
    struct PendingEvent {
        device_id: u16,
        keycode: u8,
        passthrough_keysym: Option<u32>,
    }

    #[derive(Debug, Default)]
    struct RawModifierState {
        ctrl: Vec<u8>,
        shift: Vec<u8>,
        alt: Vec<u8>,
        super_key: Vec<u8>,
    }

    impl RawModifierState {
        fn press(&mut self, keycode: u8, key: Key) {
            match key {
                Key::Control => remember_pressed_keycode(&mut self.ctrl, keycode),
                Key::Shift => remember_pressed_keycode(&mut self.shift, keycode),
                Key::Alt => remember_pressed_keycode(&mut self.alt, keycode),
                Key::Super => remember_pressed_keycode(&mut self.super_key, keycode),
                _ => {}
            }
        }

        fn release(&mut self, keycode: u8, key: Key) {
            match key {
                Key::Control => forget_pressed_keycode(&mut self.ctrl, keycode),
                Key::Shift => forget_pressed_keycode(&mut self.shift, keycode),
                Key::Alt => forget_pressed_keycode(&mut self.alt, keycode),
                Key::Super => forget_pressed_keycode(&mut self.super_key, keycode),
                _ => {}
            }
        }

        fn apply_to(&self, modifiers: &mut Modifiers) {
            // Raw press/release events are authoritative for physically held
            // modifiers. XKB may keep Ctrl+Shift latched after the desktop's
            // layout-switch shortcut, which would otherwise make every later
            // character look like a Ctrl/Shift shortcut to the input engine.
            modifiers.ctrl = !self.ctrl.is_empty();
            modifiers.shift = !self.shift.is_empty();
            modifiers.alt = !self.alt.is_empty();
            modifiers.super_key = !self.super_key.is_empty();
        }

        fn clear(&mut self) {
            self.ctrl.clear();
            self.shift.clear();
            self.alt.clear();
            self.super_key.clear();
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct PremappedTarget {
        pub keycode: u8,
        pub needs_shift: bool,
    }

    pub const VIETNAMESE_CHAR_PAIRS: &[(char, char)] = &[
        ('à', 'À'),
        ('á', 'Á'),
        ('ả', 'Ả'),
        ('ã', 'Ã'),
        ('ạ', 'Ạ'),
        ('ă', 'Ă'),
        ('ằ', 'Ằ'),
        ('ắ', 'Ắ'),
        ('ẳ', 'Ẳ'),
        ('ẵ', 'Ẵ'),
        ('ặ', 'Ặ'),
        ('â', 'Â'),
        ('ầ', 'Ầ'),
        ('ấ', 'Ấ'),
        ('ẩ', 'Ẩ'),
        ('ẫ', 'Ẫ'),
        ('ậ', 'Ậ'),
        ('đ', 'Đ'),
        ('è', 'È'),
        ('é', 'É'),
        ('ẻ', 'Ẻ'),
        ('ẽ', 'Ẽ'),
        ('ẹ', 'Ẹ'),
        ('ê', 'Ê'),
        ('ề', 'Ề'),
        ('ế', 'Ế'),
        ('ể', 'Ể'),
        ('ễ', 'Ễ'),
        ('ệ', 'Ệ'),
        ('ì', 'Ì'),
        ('í', 'Í'),
        ('ỉ', 'Ỉ'),
        ('ĩ', 'Ĩ'),
        ('ị', 'Ị'),
        ('ò', 'Ò'),
        ('ó', 'Ó'),
        ('ỏ', 'Ỏ'),
        ('õ', 'Õ'),
        ('ọ', 'Ọ'),
        ('ô', 'Ô'),
        ('ồ', 'Ồ'),
        ('ố', 'Ố'),
        ('ổ', 'Ổ'),
        ('ỗ', 'Ỗ'),
        ('ộ', 'Ộ'),
        ('ơ', 'Ơ'),
        ('ớ', 'Ớ'),
        ('ờ', 'Ờ'),
        ('ở', 'Ở'),
        ('ỡ', 'Ỡ'),
        ('ợ', 'Ợ'),
        ('ù', 'Ù'),
        ('ú', 'Ú'),
        ('ủ', 'Ủ'),
        ('ũ', 'Ũ'),
        ('ụ', 'Ụ'),
        ('ư', 'Ư'),
        ('ừ', 'Ừ'),
        ('ứ', 'Ứ'),
        ('ử', 'Ử'),
        ('ữ', 'Ữ'),
        ('ự', 'Ự'),
        ('ỳ', 'Ỳ'),
        ('ý', 'Ý'),
        ('ỷ', 'Ỷ'),
        ('ỹ', 'Ỹ'),
        ('ỵ', 'Ỵ'),
    ];

    pub const POPULAR_UPPERCASE: &[char] = &[
        'Đ', 'À', 'Á', 'Ả', 'Ã', 'Ạ', 'Â', 'Ầ', 'Ấ', 'Ẩ', 'Ẫ', 'Ậ', 'È', 'É', 'Ẻ', 'Ẽ', 'Ẹ', 'Ê',
        'Ề', 'Ế', 'Ể', 'Ễ', 'Ệ', 'Ì', 'Í', 'Ỉ', 'Ĩ', 'Ị', 'Ò', 'Ó', 'Ỏ', 'Õ', 'Ọ', 'Ô', 'Ồ', 'Ố',
        'Ổ', 'Ỗ', 'Ộ', 'Ơ', 'Ờ', 'Ớ', 'Ở', 'Ỡ', 'Ợ', 'Ù', 'Ú', 'Ủ', 'Ũ', 'Ụ', 'Ư', 'Ừ', 'Ứ', 'Ử',
        'Ữ', 'Ự', 'Ỳ', 'Ý', 'Ă', 'Ắ', 'Ằ', 'Ỷ', 'Ỹ', 'Ỵ', 'Ẳ', 'Ẵ', 'Ặ',
    ];

    #[derive(Debug)]
    pub struct PremappedChunk {
        pub first_keycode: u8,
        pub count: u8,
        pub keysyms_per_keycode: u8,
        pub original_mapping: Vec<u32>,
    }

    #[derive(Debug)]
    pub struct VietnamesePremappedTable {
        pub chunks: Vec<PremappedChunk>,
        pub char_to_target: std::collections::HashMap<char, PremappedTarget>,
        pub is_premapped: [bool; 256],
    }

    impl VietnamesePremappedTable {
        pub fn is_premapped_keycode(&self, keycode: u8) -> bool {
            self.is_premapped[usize::from(keycode)]
        }
    }

    pub struct X11KeyboardBackend {
        connection: XCBConnection,
        root: u32,
        state: xkb::State,
        keymap: xkb::Keymap,
        _context: xkb::Context,
        intercepted_keycodes: Vec<u8>,
        grab_device_ids: Vec<u16>,
        grab_modifiers: Vec<u32>,
        premapped_table: Option<VietnamesePremappedTable>,
        injection_keycode: u8,
        injection_keysyms_per_keycode: u8,
        original_injection_mapping: Vec<u32>,
        current_injection_keysym: Option<u32>,
        last_mapped_event_time: Option<Instant>,
        pending: Option<PendingEvent>,
        running: bool,
        xtest_device_ids: Vec<u16>,
        synthetic_key_presses_to_ignore: VecDeque<u8>,
        synthetic_key_releases_to_ignore: VecDeque<u8>,
        raw_modifiers: RawModifierState,
        pressed_shift_keys: Vec<(u16, u8)>,
        last_base_shift: Option<bool>,
    }

    impl std::fmt::Debug for X11KeyboardBackend {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("X11KeyboardBackend")
                .field("root", &self.root)
                .field("intercepted_keycodes", &self.intercepted_keycodes.len())
                .field("grab_device_ids", &self.grab_device_ids)
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
            ensure_us_keyboard_layout();
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
            let premapped_table = setup_premapped_table(&connection, injection_keycode)?;
            let mut excluded_keycodes = vec![injection_keycode];
            if let Some(ref table) = premapped_table {
                for (kc, &is_mapped) in table.is_premapped.iter().enumerate() {
                    if is_mapped {
                        excluded_keycodes.push(kc as u8);
                    }
                }
            }
            let intercepted_keycodes = interceptable_keycodes(&keymap, &excluded_keycodes);
            let grab_modifiers = safe_grab_modifiers(&keymap);

            info!(
                xi_major = xi_version.major_version,
                xi_minor = xi_version.minor_version,
                xtest_major = xtest_version.major_version,
                xtest_minor = xtest_version.minor_version,
                xkb_major,
                xkb_minor,
                injection_keycode,
                has_premapped = premapped_table.is_some(),
                "X11 input controller connected"
            );
            let mut xtest_device_ids = Vec::new();
            let mut grab_device_ids = Vec::new();
            if let Ok(reply) = connection.xinput_xi_query_device(u16::from(Device::ALL)) {
                if let Ok(reply_data) = reply.reply() {
                    for info in reply_data.infos {
                        if device_name_is_xtest(&info.name) {
                            xtest_device_ids.push(info.deviceid);
                        } else if should_grab_keyboard_device(info.type_, info.enabled, &info.name)
                        {
                            grab_device_ids.push(info.deviceid);
                        }
                    }
                }
            }
            debug!(
                ?xtest_device_ids,
                ?grab_device_ids,
                "Identified X11 keyboard devices"
            );

            Ok(Self {
                connection,
                root,
                state,
                keymap,
                _context: context,
                intercepted_keycodes,
                grab_device_ids,
                grab_modifiers,
                premapped_table,
                injection_keycode,
                injection_keysyms_per_keycode,
                original_injection_mapping: original_mapping,
                current_injection_keysym: None,
                last_mapped_event_time: None,
                pending: None,
                running: false,
                xtest_device_ids,
                synthetic_key_presses_to_ignore: VecDeque::new(),
                synthetic_key_releases_to_ignore: VecDeque::new(),
                raw_modifiers: RawModifierState::default(),
                pressed_shift_keys: Vec::new(),
                last_base_shift: None,
            })
        }

        fn grab_keys(&mut self) -> Result<()> {
            let mut successful_grabs = 0_usize;
            for &device_id in &self.grab_device_ids {
                successful_grabs += self.grab_keys_for_device(device_id)?;
            }
            if successful_grabs == 0 {
                return Err(KeyboardError::X11Protocol(
                    "could not grab any key on a physical X11 keyboard".to_owned(),
                ));
            }
            self.connection
                .flush()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            Ok(())
        }

        fn grab_keys_for_device(&self, device_id: u16) -> Result<usize> {
            let mask = vec![u32::from(XIEventMask::KEY_PRESS | XIEventMask::KEY_RELEASE)];
            let mut grabbed = 0_usize;

            for &keycode in &self.intercepted_keycodes {
                let reply = self
                    .connection
                    .xinput_xi_passive_grab_device(
                        CURRENT_TIME,
                        self.root,
                        NONE,
                        u32::from(keycode),
                        device_id,
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

                for failure in reply
                    .modifiers
                    .iter()
                    .filter(|item| item.status != GrabStatus::SUCCESS)
                {
                    warn!(
                        device_id,
                        keycode,
                        modifier_mask = format_args!("{:#x}", failure.modifiers),
                        status = ?failure.status,
                        "skipping X11 key grab already owned by the window manager"
                    );
                }
                // XI2 returns only modifier combinations that failed. Any
                // requested combination absent from this list was grabbed.
                if reply.modifiers.len() < self.grab_modifiers.len() {
                    grabbed += 1;
                }
            }
            Ok(grabbed)
        }

        fn ungrab_keycodes(&self) {
            for &device_id in &self.grab_device_ids {
                for &keycode in &self.intercepted_keycodes {
                    match self.connection.xinput_xi_passive_ungrab_device(
                        self.root,
                        u32::from(keycode),
                        device_id,
                        GrabType::KEYCODE,
                        &self.grab_modifiers,
                    ) {
                        Ok(cookie) => {
                            if let Err(error) = cookie.check() {
                                warn!(device_id, keycode, %error, "failed to remove passive X11 key grab");
                            }
                        }
                        Err(error) => {
                            warn!(device_id, keycode, %error, "failed to send passive X11 ungrab request");
                        }
                    }
                }
            }
        }

        fn refresh_grab_devices(&mut self) -> Result<()> {
            let reply = self
                .connection
                .xinput_xi_query_device(u16::from(Device::ALL))
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
                .reply()
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            let current = reply
                .infos
                .into_iter()
                .filter(|info| should_grab_keyboard_device(info.type_, info.enabled, &info.name))
                .map(|info| info.deviceid)
                .collect::<Vec<_>>();

            for &device_id in &current {
                if !self.grab_device_ids.contains(&device_id) {
                    let grabbed = self.grab_keys_for_device(device_id)?;
                    info!(device_id, grabbed, "grabbed newly attached X11 keyboard");
                }
            }
            self.grab_device_ids = current;
            self.connection
                .flush()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))
        }

        fn decode(&mut self, event: &KeyPressEvent) -> Result<KeyEvent> {
            let keycode = u8::try_from(event.detail).map_err(|_| {
                KeyboardError::X11Protocol(format!(
                    "XInput2 returned invalid keycode {}",
                    event.detail
                ))
            })?;
            let event_base_shift =
                modifier_is_active(&self.keymap, xkb::MOD_NAME_SHIFT, event.mods.base);
            let had_tracked_shift = !self.raw_modifiers.shift.is_empty();
            let mut shift_held = self.reconcile_shift_with_physical_devices();
            let released_stale_shift = had_tracked_shift && !shift_held;
            let hidden_shift_press = !shift_held
                && !released_stale_shift
                && event_base_shift
                && match self.last_base_shift {
                    Some(previous) => !previous,
                    None => self.device_shift_is_held(event.sourceid).unwrap_or(true),
                };
            if hidden_shift_press {
                self.remember_grabbed_shift_press(event.sourceid);
                shift_held = true;
                debug!(
                    device_id = event.sourceid,
                    "recovered Shift press hidden by an active X11 grab"
                );
            }
            let (base_mods, latched_mods) =
                self.normalize_stale_shift_state(event.mods.base, event.mods.latched, shift_held);
            self.last_base_shift = Some(modifier_is_active(
                &self.keymap,
                xkb::MOD_NAME_SHIFT,
                base_mods,
            ));
            self.state.update_mask(
                base_mods,
                latched_mods,
                event.mods.locked,
                u32::from(event.group.base),
                u32::from(event.group.latched),
                u32::from(event.group.locked),
            );
            let key = self.raw_key_for_keycode(keycode);
            if hidden_shift_press {
                if let (Some(pending), Key::Character(character)) = (self.pending.as_mut(), key) {
                    pending.passthrough_keysym = Some(Keysym::from_char(character).raw());
                }
            }
            let mut modifiers = self.modifiers(event.mods.effective);
            // Desktop layout switching can leave Shift latched in the XKB
            // snapshot even after the physical key is released. Raw modifier
            // tracking remains the authoritative source for what is held now.
            self.raw_modifiers.apply_to(&mut modifiers);
            let decoded = KeyEvent::press(key);
            let decoded = KeyEvent {
                modifiers,
                ..decoded
            };
            debug!(
                ?decoded,
                keycode,
                base_mods = format_args!("{:#x}", event.mods.base),
                latched_mods = format_args!("{:#x}", event.mods.latched),
                locked_mods = format_args!("{:#x}", event.mods.locked),
                effective_mods = format_args!("{:#x}", event.mods.effective),
                "captured X11 key event"
            );
            Ok(decoded)
        }

        fn normalize_stale_shift_state(
            &mut self,
            mut base_mods: u32,
            mut latched_mods: u32,
            shift_held: bool,
        ) -> (u32, u32) {
            let modifier_index = self.keymap.mod_get_index(xkb::MOD_NAME_SHIFT);

            if let Some(mask) = stale_modifier_mask(modifier_index, base_mods, shift_held) {
                match self.release_stale_shift_keys() {
                    Ok(()) => base_mods &= !u32::from(mask),
                    Err(error) => warn!(%error, "failed to release stale X11 Shift state"),
                }
            }

            let Some(mask) = stale_modifier_mask(modifier_index, latched_mods, shift_held) else {
                return (base_mods, latched_mods);
            };

            let clear_result = self
                .connection
                .xkb_latch_lock_state(
                    XkbDeviceId::USE_CORE_KBD.into(),
                    xproto::ModMask::default(),
                    xproto::ModMask::default(),
                    false,
                    0_u8.into(),
                    mask.into(),
                    false,
                    0,
                )
                .map_err(|error| error.to_string())
                .and_then(|cookie| cookie.check().map_err(|error| error.to_string()));

            match clear_result {
                Ok(()) => {
                    debug!(
                        modifier_mask = format_args!("{mask:#x}"),
                        "cleared stale X11 Shift latch"
                    );
                    latched_mods &= !u32::from(mask);
                }
                Err(error) => {
                    warn!(%error, "failed to clear stale X11 Shift latch");
                }
            }
            (base_mods, latched_mods)
        }

        fn release_stale_shift_keys(&mut self) -> Result<()> {
            let keycodes = modifier_keycodes(&self.keymap, Key::Shift);
            for &keycode in &keycodes {
                self.queue_synthetic_release(keycode)?;
            }
            self.connection
                .sync()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            debug!(?keycodes, "released stale X11 Shift key state");
            Ok(())
        }

        fn repair_stale_shift_state_on_shutdown(&mut self) {
            let state = self
                .connection
                .xkb_get_state(XkbDeviceId::USE_CORE_KBD.into())
                .map_err(|error| error.to_string())
                .and_then(|cookie| cookie.reply().map_err(|error| error.to_string()));
            let state = match state {
                Ok(state) => state,
                Err(error) => {
                    warn!(%error, "failed to query XKB state during shutdown");
                    return;
                }
            };
            let shift_held = self
                .physical_shift_is_held()
                .unwrap_or(!self.raw_modifiers.shift.is_empty());
            self.normalize_stale_shift_state(
                state.base_mods.into(),
                state.latched_mods.into(),
                shift_held,
            );
        }

        fn physical_shift_is_held(&self) -> Option<bool> {
            let shift_keycodes = modifier_keycodes(&self.keymap, Key::Shift);
            let mut queried_keyboard = false;

            for &device_id in &self.grab_device_ids {
                let Ok(device_id) = u8::try_from(device_id) else {
                    continue;
                };
                let reply = match self.connection.xinput_query_device_state(device_id) {
                    Ok(cookie) => match cookie.reply() {
                        Ok(reply) => reply,
                        Err(error) => {
                            warn!(device_id, %error, "failed to read physical keyboard state");
                            continue;
                        }
                    },
                    Err(error) => {
                        warn!(device_id, %error, "failed to query physical keyboard state");
                        continue;
                    }
                };

                for class in reply.classes {
                    let InputStateData::Key(key_state) = class.data else {
                        continue;
                    };
                    queried_keyboard = true;
                    if shift_keycodes
                        .iter()
                        .any(|keycode| keycode_is_pressed(&key_state.keys, *keycode))
                    {
                        return Some(true);
                    }
                }
            }

            queried_keyboard.then_some(false)
        }

        fn reconcile_shift_with_physical_devices(&mut self) -> bool {
            if self.raw_modifiers.shift.is_empty() {
                self.pressed_shift_keys.clear();
                return false;
            }
            if self.pressed_shift_keys.is_empty() {
                return true;
            }

            let tracked = self.pressed_shift_keys.clone();
            let mut queried_device = false;
            for &(device_id, keycode) in &tracked {
                let Ok(device_id) = u8::try_from(device_id) else {
                    continue;
                };
                let reply = match self.connection.xinput_query_device_state(device_id) {
                    Ok(cookie) => match cookie.reply() {
                        Ok(reply) => reply,
                        Err(_) => continue,
                    },
                    Err(_) => continue,
                };
                for class in reply.classes {
                    let InputStateData::Key(key_state) = class.data else {
                        continue;
                    };
                    queried_device = true;
                    if keycode_is_pressed(&key_state.keys, keycode) {
                        return true;
                    }
                }
            }

            if queried_device {
                self.raw_modifiers.shift.clear();
                self.pressed_shift_keys.clear();
                if let Err(error) = self.release_stale_shift_keys() {
                    warn!(%error, "failed to publish recovered X11 Shift release");
                }
                debug!("reconciled released Shift hidden by an active X11 grab");
                false
            } else {
                true
            }
        }

        fn remember_grabbed_shift_press(&mut self, device_id: u16) {
            for keycode in modifier_keycodes(&self.keymap, Key::Shift) {
                self.raw_modifiers.press(keycode, Key::Shift);
                remember_pressed_shift(&mut self.pressed_shift_keys, device_id, keycode);
            }
        }

        fn device_shift_is_held(&self, device_id: u16) -> Option<bool> {
            let device_id = u8::try_from(device_id).ok()?;
            let reply = self
                .connection
                .xinput_query_device_state(device_id)
                .ok()?
                .reply()
                .ok()?;
            let shift_keycodes = modifier_keycodes(&self.keymap, Key::Shift);
            reply.classes.into_iter().find_map(|class| {
                let InputStateData::Key(key_state) = class.data else {
                    return None;
                };
                Some(
                    shift_keycodes
                        .iter()
                        .any(|keycode| keycode_is_pressed(&key_state.keys, *keycode)),
                )
            })
        }

        fn decode_raw(&mut self, event: &RawKeyPressEvent) -> Result<KeyEvent> {
            let keycode = u8::try_from(event.detail).map_err(|_| {
                KeyboardError::X11Protocol(format!(
                    "XInput2 returned invalid raw keycode {}",
                    event.detail
                ))
            })?;
            let xkb_keycode = xkb::Keycode::new(keycode.into());
            let key = self.raw_key_for_keycode(keycode);
            self.raw_modifiers.press(keycode, key);
            if key == Key::Shift {
                remember_pressed_shift(&mut self.pressed_shift_keys, event.sourceid, keycode);
            }
            let mut modifiers = self.current_modifiers();
            self.raw_modifiers.apply_to(&mut modifiers);
            let decoded = KeyEvent::press(key);
            let decoded = KeyEvent {
                modifiers,
                ..decoded
            };
            self.state.update_key(xkb_keycode, xkb::KeyDirection::Down);
            debug!(?decoded, keycode, "observed X11 raw key press");
            Ok(decoded)
        }

        fn raw_key_for_keycode(&self, keycode: u8) -> Key {
            if let Some(key) = modifier_key_for_keycode(&self.keymap, keycode) {
                return key;
            }

            let xkb_keycode = xkb::Keycode::new(keycode.into());
            let caps_lock = self
                .state
                .mod_name_is_active(xkb::MOD_NAME_CAPS, xkb::STATE_MODS_LOCKED);
            let shift_held = !self.raw_modifiers.shift.is_empty();
            let keysym = prefer_unshifted_keysym(
                self.state.key_get_one_sym(xkb_keycode).raw(),
                self.unshifted_keysym_for_keycode(xkb_keycode),
                shift_held,
                caps_lock,
            );

            key_from_keysym(keysym)
        }

        fn unshifted_keysym_for_keycode(&self, keycode: xkb::Keycode) -> Option<u32> {
            let layout = self.state.key_get_layout(keycode);
            self.keymap
                .key_get_syms_by_level(keycode, layout, 0)
                .first()
                .map(|keysym| keysym.raw())
        }

        fn decode_raw_release(&mut self, event: &RawKeyPressEvent) -> Result<Option<KeyEvent>> {
            let keycode = u8::try_from(event.detail).map_err(|_| {
                KeyboardError::X11Protocol(format!(
                    "XInput2 returned invalid raw keycode {}",
                    event.detail
                ))
            })?;
            let key = modifier_key_for_keycode(&self.keymap, keycode);
            if let Some(key) = key {
                self.raw_modifiers.release(keycode, key);
                if key == Key::Shift {
                    forget_pressed_shift(&mut self.pressed_shift_keys, event.sourceid, keycode);
                }
            }
            self.state
                .update_key(xkb::Keycode::new(keycode.into()), xkb::KeyDirection::Up);
            let Some(key) = key else {
                return Ok(None);
            };
            let mut modifiers = self.current_modifiers();
            self.raw_modifiers.apply_to(&mut modifiers);
            let decoded = KeyEvent {
                key,
                modifiers,
                state: KeyState::Release,
            };
            debug!(?decoded, keycode, "observed X11 raw modifier release");
            Ok(Some(decoded))
        }

        fn decode_grabbed_modifier_release(
            &mut self,
            event: &KeyPressEvent,
        ) -> Result<Option<KeyEvent>> {
            let keycode = u8::try_from(event.detail).map_err(|_| {
                KeyboardError::X11Protocol(format!(
                    "XInput2 returned invalid release keycode {}",
                    event.detail
                ))
            })?;
            let Some(key) = modifier_key_for_keycode(&self.keymap, keycode) else {
                return Ok(None);
            };

            self.raw_modifiers.release(keycode, key);
            if key == Key::Shift {
                forget_pressed_shift(&mut self.pressed_shift_keys, event.sourceid, keycode);
            }
            // If a modifier is released while another passively grabbed key
            // is still down, XI2 delivers the release only to the grab owner.
            // Publish the same release through XTEST so the focused client
            // cannot retain a pressed modifier after VKey handled it.
            self.queue_synthetic_release(keycode)?;
            self.connection
                .sync()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            self.state
                .update_key(xkb::Keycode::new(keycode.into()), xkb::KeyDirection::Up);
            let mut modifiers = self.modifiers(event.mods.effective);
            self.raw_modifiers.apply_to(&mut modifiers);
            let decoded = KeyEvent {
                key,
                modifiers,
                state: KeyState::Release,
            };
            debug!(?decoded, keycode, "observed X11 grabbed modifier release");
            Ok(Some(decoded))
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
            self.replace_text(0, text)
        }

        pub(crate) fn delete_graphemes(&mut self, count: usize) -> Result<()> {
            self.replace_text(count, "")
        }

        pub(crate) fn replace_text(&mut self, delete_graphemes: usize, text: &str) -> Result<()> {
            if delete_graphemes == 0 && text.is_empty() {
                return Ok(());
            }

            let _target = self.require_focused_window()?;

            // 1. If we have Backspaces, send them first and sync so the application deletes cleanly
            if delete_graphemes > 0 {
                let backspace = find_direct_keycode_in(&self.keymap, key::BackSpace)
                    .map_or(SyntheticKey::Mapped(key::BackSpace), SyntheticKey::Direct);
                for _ in 0..delete_graphemes {
                    self.queue_synthetic_key(backspace)?;
                }
                self.connection
                    .sync()
                    .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
                std::thread::sleep(Duration::from_millis(2));
            }

            // 2. Send replacement characters
            for character in text.chars() {
                let key = if let Some(ref table) = self.premapped_table {
                    if let Some(&target) = table.char_to_target.get(&character) {
                        SyntheticKey::Premapped {
                            keycode: target.keycode,
                            needs_shift: target.needs_shift,
                        }
                    } else {
                        let keysym = Keysym::from_char(character).raw();
                        find_direct_keycode_in(&self.keymap, keysym)
                            .map_or(SyntheticKey::Mapped(keysym), SyntheticKey::Direct)
                    }
                } else {
                    let keysym = Keysym::from_char(character).raw();
                    find_direct_keycode_in(&self.keymap, keysym)
                        .map_or(SyntheticKey::Mapped(keysym), SyntheticKey::Direct)
                };
                self.queue_synthetic_key(key)?;
            }

            self.connection
                .sync()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))
        }

        #[allow(dead_code)]
        fn find_direct_keycode(&self, target_keysym: u32) -> Option<u8> {
            find_direct_keycode_in(&self.keymap, target_keysym)
        }

        fn require_focused_window(&self) -> Result<WindowId> {
            self.focused_window()?.ok_or(KeyboardError::NoFocusedWindow)
        }

        fn queue_synthetic_key(&mut self, key: SyntheticKey) -> Result<()> {
            let is_mapped = match key {
                SyntheticKey::Direct(keycode) => {
                    queue_fake_key_on(&self.connection, keycode)?;
                    self.synthetic_key_presses_to_ignore.push_back(keycode);
                    self.synthetic_key_releases_to_ignore.push_back(keycode);
                    false
                }
                SyntheticKey::Premapped {
                    keycode,
                    needs_shift,
                } => {
                    if needs_shift {
                        let shift_kc =
                            find_direct_keycode_in(&self.keymap, key::Shift_L).unwrap_or(50);
                        self.queue_synthetic_press(shift_kc)?;
                        queue_fake_key_on(&self.connection, keycode)?;
                        self.synthetic_key_presses_to_ignore.push_back(keycode);
                        self.synthetic_key_releases_to_ignore.push_back(keycode);
                        self.queue_synthetic_release(shift_kc)?;
                    } else {
                        queue_fake_key_on(&self.connection, keycode)?;
                        self.synthetic_key_presses_to_ignore.push_back(keycode);
                        self.synthetic_key_releases_to_ignore.push_back(keycode);
                    }
                    false
                }
                SyntheticKey::Mapped(keysym) => {
                    self.set_injection_keysym(keysym)?;
                    let keycode = self.injection_keycode;
                    queue_fake_key_on(&self.connection, keycode)?;
                    self.synthetic_key_presses_to_ignore.push_back(keycode);
                    self.synthetic_key_releases_to_ignore.push_back(keycode);
                    true
                }
            };
            if is_mapped {
                self.last_mapped_event_time = Some(Instant::now());
            }
            Ok(())
        }

        fn queue_synthetic_press(&mut self, keycode: u8) -> Result<()> {
            let _cookie = self
                .connection
                .xtest_fake_input(
                    xproto::KEY_PRESS_EVENT,
                    keycode,
                    CURRENT_TIME,
                    NONE,
                    0,
                    0,
                    0,
                )
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            self.synthetic_key_presses_to_ignore.push_back(keycode);
            Ok(())
        }

        fn queue_synthetic_release(&mut self, keycode: u8) -> Result<()> {
            let _cookie = self
                .connection
                .xtest_fake_input(
                    xproto::KEY_RELEASE_EVENT,
                    keycode,
                    CURRENT_TIME,
                    NONE,
                    0,
                    0,
                    0,
                )
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            self.synthetic_key_releases_to_ignore.push_back(keycode);
            Ok(())
        }

        fn set_injection_keysym(&mut self, keysym: u32) -> Result<()> {
            if self.current_injection_keysym == Some(keysym) {
                return Ok(());
            }
            // The focused client resolves keycodes using its own cached keymap.
            // Let it consume earlier XTEST events before this keycode is remapped.
            self.settle_pending_mapped_keys()?;
            let keysyms = vec![keysym; usize::from(self.injection_keysyms_per_keycode)];
            self.connection
                .change_keyboard_mapping(
                    1,
                    self.injection_keycode,
                    self.injection_keysyms_per_keycode,
                    &keysyms,
                )
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
                .check()
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            self.current_injection_keysym = Some(keysym);
            self.connection
                .sync()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            Ok(())
        }

        fn settle_pending_mapped_keys(&mut self) -> Result<()> {
            let Some(sent_at) = self.last_mapped_event_time.take() else {
                return Ok(());
            };
            let elapsed = sent_at.elapsed();
            if elapsed < Duration::from_millis(3) {
                let remaining = Duration::from_millis(3) - elapsed;
                self.connection
                    .sync()
                    .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
                std::thread::sleep(remaining);
            }
            Ok(())
        }

        fn restore_premapped_table(&mut self) -> Result<()> {
            if let Some(table) = self.premapped_table.take() {
                for chunk in table.chunks {
                    let _cookie = self
                        .connection
                        .change_keyboard_mapping(
                            chunk.count,
                            chunk.first_keycode,
                            chunk.keysyms_per_keycode,
                            &chunk.original_mapping,
                        )
                        .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
                }
                self.connection
                    .flush()
                    .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
                info!("Restored original X11 keycode mappings for pre-mapped chunks");
            }
            Ok(())
        }

        fn restore_injection_mapping(&mut self) -> Result<()> {
            if self.current_injection_keysym.is_none() {
                return Ok(());
            }
            self.settle_pending_mapped_keys()?;
            let _cookie = self
                .connection
                .change_keyboard_mapping(
                    1,
                    self.injection_keycode,
                    self.injection_keysyms_per_keycode,
                    &self.original_injection_mapping,
                )
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            self.connection
                .flush()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            self.current_injection_keysym = None;
            Ok(())
        }

        fn take_queued_synthetic_press(&mut self, detail: u32) -> bool {
            take_queued_synthetic_key(&mut self.synthetic_key_presses_to_ignore, detail)
        }

        fn take_queued_synthetic_release(&mut self, detail: u32) -> bool {
            take_queued_synthetic_key(&mut self.synthetic_key_releases_to_ignore, detail)
        }

        fn is_synthetic_event(&self, device_id: u16, source_id: u16, detail: u32) -> bool {
            synthetic_event_is_identified(
                &self.xtest_device_ids,
                self.premapped_table.as_ref(),
                device_id,
                source_id,
                detail,
                self.injection_keycode,
            )
        }

        fn allow_event(&self, pending: PendingEvent, mode: EventMode) -> Result<()> {
            self.connection
                .xinput_xi_allow_events(CURRENT_TIME, pending.device_id, mode, 0, self.root)
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            Ok(())
        }

        fn wait_for_event_until(&self, deadline: Instant) -> Result<Option<Event>> {
            loop {
                if let Some(event) = self
                    .connection
                    .poll_for_event()
                    .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?
                {
                    return Ok(Some(event));
                }

                let now = Instant::now();
                if now >= deadline {
                    return Ok(None);
                }
                let timeout_ms = deadline
                    .saturating_duration_since(now)
                    .as_millis()
                    .clamp(1, i32::MAX as u128) as i32;
                let mut descriptor = libc::pollfd {
                    fd: self.connection.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: descriptor points to one initialized pollfd for the
                // duration of this call, and the XCB connection owns the fd.
                let ready = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
                if ready < 0 {
                    let error = std::io::Error::last_os_error();
                    if error.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(KeyboardError::ConnectionLost(error.to_string()));
                }
                if ready == 0 {
                    return Ok(None);
                }
                if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                    return Err(KeyboardError::ConnectionLost(format!(
                        "X11 connection poll failed with flags {:#x}",
                        descriptor.revents
                    )));
                }
            }
        }
    }

    impl KeyboardBackend for X11KeyboardBackend {
        fn start(&mut self) -> Result<()> {
            if self.running {
                return Ok(());
            }
            self.grab_keys()?;
            let masks = [
                EventMask {
                    deviceid: Device::ALL_MASTER.into(),
                    mask: vec![
                        XIEventMask::RAW_KEY_PRESS
                            | XIEventMask::RAW_KEY_RELEASE
                            | XIEventMask::RAW_BUTTON_PRESS,
                    ],
                },
                EventMask {
                    deviceid: Device::ALL.into(),
                    mask: vec![XIEventMask::HIERARCHY],
                },
            ];
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
                devices = self.grab_device_ids.len(),
                "X11 keyboard interception started"
            );
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            if !self.running {
                return Ok(());
            }
            // Repair server-side modifier state before any fallible shutdown
            // step so a normal exit cannot leave Shift depressed or latched.
            self.repair_stale_shift_state_on_shutdown();
            let masks = [
                EventMask {
                    deviceid: Device::ALL_MASTER.into(),
                    mask: Vec::new(),
                },
                EventMask {
                    deviceid: Device::ALL.into(),
                    mask: Vec::new(),
                },
            ];
            self.connection
                .xinput_xi_select_events(self.root, &masks)
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?
                .check()
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
            if let Some(pending) = self.pending.take() {
                let _ = self.allow_event(pending, EventMode::ASYNC_DEVICE);
            }
            self.ungrab_keycodes();
            self.connection
                .flush()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))?;
            let r1 = self.restore_injection_mapping();
            let r2 = self.restore_premapped_table();
            r1?;
            r2?;
            self.raw_modifiers.clear();
            self.pressed_shift_keys.clear();
            self.last_base_shift = None;
            self.running = false;
            info!("X11 keyboard interception stopped");
            Ok(())
        }

        fn next_event(&mut self) -> Result<KeyEvent> {
            if !self.running {
                return Err(KeyboardError::NotRunning);
            }
            let deadline = Instant::now() + EVENT_WAIT_TIMEOUT;
            loop {
                let event = match self.wait_for_event_until(deadline)? {
                    Some(event) => event,
                    None => return Err(KeyboardError::Timeout),
                };
                match event {
                    Event::XinputRawKeyPress(event) => {
                        if self.is_synthetic_event(event.deviceid, event.sourceid, event.detail) {
                            let _ = self.take_queued_synthetic_press(event.detail);
                            trace!(
                                keycode = event.detail,
                                device_id = event.deviceid,
                                source_id = event.sourceid,
                                "ignored synthetic X11 raw key press"
                            );
                            continue;
                        }
                        let decoded = self.decode_raw(&event)?;
                        if is_raw_only_key(decoded.key) {
                            return Ok(decoded);
                        }
                    }
                    Event::XinputRawKeyRelease(event) => {
                        if self.is_synthetic_event(event.deviceid, event.sourceid, event.detail) {
                            let _ = self.take_queued_synthetic_release(event.detail);
                            trace!(
                                keycode = event.detail,
                                device_id = event.deviceid,
                                source_id = event.sourceid,
                                "ignored synthetic X11 raw key release"
                            );
                            continue;
                        }
                        if let Some(decoded) = self.decode_raw_release(&event)? {
                            return Ok(decoded);
                        }
                    }
                    Event::XinputRawButtonPress(event) => {
                        // Pointer buttons 1-3 can move the caret or focus to a
                        // different text field. Emit a boundary event so the
                        // composition buffer cannot rewrite text at the old
                        // caret. Wheel buttons do not move the caret.
                        if let Some(boundary) = pointer_click_boundary(event.detail) {
                            debug!(
                                button = event.detail,
                                "observed X11 pointer click; clearing composition"
                            );
                            return Ok(boundary);
                        }
                    }
                    Event::XinputHierarchy(_) => {
                        self.refresh_grab_devices()?;
                    }
                    Event::XinputKeyPress(event) => {
                        let pending = PendingEvent {
                            device_id: event.deviceid,
                            keycode: u8::try_from(event.detail).map_err(|_| {
                                KeyboardError::X11Protocol(format!(
                                    "XInput2 returned invalid keycode {}",
                                    event.detail
                                ))
                            })?,
                            passthrough_keysym: None,
                        };
                        if self.is_synthetic_event(event.deviceid, event.sourceid, event.detail) {
                            self.allow_event(pending, EventMode::ASYNC_DEVICE)?;
                            self.connection.flush().map_err(|error| {
                                KeyboardError::ConnectionLost(error.to_string())
                            })?;
                            trace!(
                                keycode = event.detail,
                                device_id = event.deviceid,
                                source_id = event.sourceid,
                                "released synthetic event from X11 passive grab"
                            );
                            continue;
                        }
                        self.pending = Some(pending);
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
                        if let Some(decoded) = self.decode_grabbed_modifier_release(&event)? {
                            return Ok(decoded);
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
            let Some(pending) = self.pending.take() else {
                return Ok(());
            };

            self.allow_event(pending, EventMode::ASYNC_DEVICE)?;
            if decision == KeyboardDecision::PassThrough {
                let key = pending
                    .passthrough_keysym
                    .map_or(SyntheticKey::Direct(pending.keycode), SyntheticKey::Mapped);
                self.queue_synthetic_key(key)?;
            }
            self.connection
                .flush()
                .map_err(|error| KeyboardError::ConnectionLost(error.to_string()))
        }

        fn is_running(&self) -> bool {
            self.running
        }
    }

    impl Drop for X11KeyboardBackend {
        fn drop(&mut self) {
            if self.running {
                let _ = self.stop();
            } else {
                if let Err(error) = self.restore_injection_mapping() {
                    warn!(%error, "failed to restore temporary X11 keyboard mapping");
                }
                if let Err(error) = self.restore_premapped_table() {
                    warn!(%error, "failed to restore pre-mapped X11 keyboard mapping");
                }
            }
        }
    }

    fn is_vital_key(kc: u8) -> bool {
        matches!(
            kc,
            9..=22 // Esc, 1..=, Backspace
            | 23 // Tab
            | 24..=36 // q..], Enter
            | 37 // Control_L
            | 38..=48 // a..'
            | 49 // `~
            | 50 // Shift_L
            | 51 // \|
            | 52..=61 // z../
            | 62 // Shift_R
            | 63 // KP_Multiply
            | 64 // Alt_L
            | 65 // Space
            | 66 // Caps_Lock
            | 67..=76 // F1..F10
            | 77 // Num_Lock
            | 78 // Scroll_Lock
            | 79..=91 // Numpad
            | 95..=96 // F11, F12
            | 104..=108 // KP_Enter, Control_R, KP_Divide, Print, Alt_R
            | 110..=119 // Home, Up, Prior, Left, Right, End, Down, Next, Insert, Delete
            | 121..=124 // XF86AudioMute, LowerVolume, RaiseVolume, PowerOff
            | 127 // Pause
            | 133..=135 // Super_L, Super_R, Menu
            | 203..=207 // Modifiers: Mode_switch, Alt_L, Meta_L, Super_L, Hyper_L
            | 218 // Print
            | 232..=233 // MonBrightnessDown, MonBrightnessUp
        )
    }

    struct KeycodeMappingPlan {
        keycode: u8,
        keysyms: Vec<u32>,
    }

    fn setup_premapped_table(
        connection: &XCBConnection,
        reserved_injection_keycode: u8,
    ) -> Result<Option<VietnamesePremappedTable>> {
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

        let width = reply.keysyms_per_keycode;
        if width == 0 {
            warn!("X11 returned zero keysyms per keycode");
            return Ok(None);
        }

        let is_empty_kc = |kc: u8| -> bool {
            let offset = (kc - min) as usize;
            let start = offset * (width as usize);
            let end = start + (width as usize);
            if end <= reply.keysyms.len() {
                reply.keysyms[start..end].iter().all(|&sym| sym == 0)
            } else {
                false
            }
        };

        let mut candidate_keycodes: Vec<u8> = Vec::with_capacity(140);

        // 1. Completely empty keycodes (all keysyms == 0)
        for kc in min..=max {
            if !is_vital_key(kc)
                && kc != reserved_injection_keycode
                && kc != 8
                && is_empty_kc(kc)
                && !candidate_keycodes.contains(&kc)
            {
                candidate_keycodes.push(kc);
            }
        }

        // 2. Unused Asian / Japanese keys (not on US/standard keyboards)
        for kc in [98, 99, 100, 101, 102, 130, 131] {
            if kc >= min
                && kc <= max
                && !is_vital_key(kc)
                && kc != reserved_injection_keycode
                && !candidate_keycodes.contains(&kc)
            {
                candidate_keycodes.push(kc);
            }
        }

        // 3. Spare / Unused non-vital keys
        for kc in [93, 97, 103, 109, 120, 125, 126, 128, 129, 132] {
            if kc >= min
                && kc <= max
                && !is_vital_key(kc)
                && kc != reserved_injection_keycode
                && !candidate_keycodes.contains(&kc)
            {
                candidate_keycodes.push(kc);
            }
        }

        // 4. Safe high keycodes (XF86 media / launch / extra keys)
        let safe_ranges = [136..=202, 208..=217, 219..=231, 234..=247, 249..=255];
        for range in safe_ranges {
            for kc in range {
                if kc >= min
                    && kc <= max
                    && !is_vital_key(kc)
                    && kc != reserved_injection_keycode
                    && !candidate_keycodes.contains(&kc)
                {
                    candidate_keycodes.push(kc);
                }
            }
        }

        // 5. Safe ISO / low keycode if still needed
        for kc in [94, 8] {
            if kc >= min
                && kc <= max
                && !is_vital_key(kc)
                && kc != reserved_injection_keycode
                && !candidate_keycodes.contains(&kc)
            {
                candidate_keycodes.push(kc);
            }
        }

        if candidate_keycodes.is_empty() {
            warn!("X11 has no spare keycodes available for pre-mapped Vietnamese keymap");
            return Ok(None);
        }

        let total_candidates = candidate_keycodes.len();
        let pair_count = VIETNAMESE_CHAR_PAIRS.len();
        let lower_count = pair_count.min(total_candidates);

        let mut plans = Vec::with_capacity(total_candidates);
        let mut char_to_target = std::collections::HashMap::with_capacity(pair_count * 2);
        let mut is_premapped = [false; 256];

        // Map pairs (lower in Level 0, upper in Level 1)
        for idx in 0..lower_count {
            let kc = candidate_keycodes[idx];
            let (lower, upper) = VIETNAMESE_CHAR_PAIRS[idx];
            let lower_sym = Keysym::from_char(lower).raw();
            let upper_sym = Keysym::from_char(upper).raw();

            let mut keysyms = Vec::with_capacity(width as usize);
            for slot in 0..width {
                if slot % 2 == 0 {
                    keysyms.push(lower_sym);
                } else {
                    keysyms.push(upper_sym);
                }
            }
            plans.push(KeycodeMappingPlan {
                keycode: kc,
                keysyms,
            });
            char_to_target.insert(
                lower,
                PremappedTarget {
                    keycode: kc,
                    needs_shift: false,
                },
            );
            // By default, upper is reachable via Shift on this keycode
            char_to_target.insert(
                upper,
                PremappedTarget {
                    keycode: kc,
                    needs_shift: true,
                },
            );
            is_premapped[usize::from(kc)] = true;
        }

        // Assign dedicated unshifted keycodes to uppercase characters where available
        if total_candidates > lower_count {
            let extra_count = total_candidates - lower_count;
            for (offset, &upper_char) in POPULAR_UPPERCASE.iter().take(extra_count).enumerate() {
                let kc = candidate_keycodes[lower_count + offset];
                let upper_sym = Keysym::from_char(upper_char).raw();
                let keysyms = vec![upper_sym; width as usize];
                plans.push(KeycodeMappingPlan {
                    keycode: kc,
                    keysyms,
                });
                char_to_target.insert(
                    upper_char,
                    PremappedTarget {
                        keycode: kc,
                        needs_shift: false,
                    },
                );
                is_premapped[usize::from(kc)] = true;
            }
        }

        // Sort plans by keycode so we can create contiguous chunks
        plans.sort_by_key(|p| p.keycode);

        // Group into contiguous chunks
        let mut chunks = Vec::new();
        let mut i = 0;
        while i < plans.len() {
            let first_kc = plans[i].keycode;
            let mut cnt = 1_u8;
            while i + (cnt as usize) < plans.len()
                && plans[i + (cnt as usize)].keycode == first_kc + cnt
            {
                cnt += 1;
            }

            let chunk_reply = match connection.get_keyboard_mapping(first_kc, cnt) {
                Ok(cookie) => match cookie.reply() {
                    Ok(reply) => reply,
                    Err(error) => {
                        rollback_chunks(connection, &chunks);
                        return Err(KeyboardError::X11Protocol(error.to_string()));
                    }
                },
                Err(error) => {
                    rollback_chunks(connection, &chunks);
                    return Err(KeyboardError::X11Protocol(error.to_string()));
                }
            };
            let orig_mapping = chunk_reply.keysyms;

            let mut new_mapping = Vec::with_capacity((cnt as usize) * (width as usize));
            for offset in 0..cnt {
                new_mapping.extend_from_slice(&plans[i + (offset as usize)].keysyms);
            }

            match connection.change_keyboard_mapping(cnt, first_kc, width, &new_mapping) {
                Ok(cookie) => {
                    if let Err(error) = cookie.check() {
                        rollback_chunks(connection, &chunks);
                        return Err(KeyboardError::X11Protocol(error.to_string()));
                    }
                }
                Err(error) => {
                    rollback_chunks(connection, &chunks);
                    return Err(KeyboardError::X11Protocol(error.to_string()));
                }
            }

            chunks.push(PremappedChunk {
                first_keycode: first_kc,
                count: cnt,
                keysyms_per_keycode: chunk_reply.keysyms_per_keycode,
                original_mapping: orig_mapping,
            });

            i += cnt as usize;
        }

        info!(
            chunk_count = chunks.len(),
            mapped_keycodes = plans.len(),
            mapped_chars = char_to_target.len(),
            "Pre-mapped Vietnamese Unicode characters to safe X11 keycodes"
        );

        Ok(Some(VietnamesePremappedTable {
            chunks,
            char_to_target,
            is_premapped,
        }))
    }

    fn rollback_chunks(connection: &XCBConnection, chunks: &[PremappedChunk]) {
        for chunk in chunks {
            let _ = connection.change_keyboard_mapping(
                chunk.count,
                chunk.first_keycode,
                chunk.keysyms_per_keycode,
                &chunk.original_mapping,
            );
        }
        let _ = connection.flush();
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

        // First preference: an unused keycode where all keysyms are zero
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

        // Fallback: if no completely empty keycode exists, borrow a safe high keycode (e.g. max keycode)
        let fallback_keycode = max;
        let offset = usize::from(fallback_keycode.saturating_sub(min));
        let start = offset * width;
        let end = start + width;
        if end <= reply.keysyms.len() {
            let orig = reply.keysyms[start..end].to_vec();
            warn!(
                fallback_keycode,
                "no empty keycode found; using fallback high keycode for Unicode injection"
            );
            return Ok((fallback_keycode, reply.keysyms_per_keycode, orig));
        }

        Err(KeyboardError::NoSpareKeycode)
    }

    fn interceptable_keycodes(keymap: &xkb::Keymap, excluded: &[u8]) -> Vec<u8> {
        let min = keymap.min_keycode().raw();
        let max = keymap.max_keycode().raw();
        (min..=max)
            .filter_map(|raw| u8::try_from(raw).ok())
            .filter(|keycode| !excluded.contains(keycode))
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
            return false;
        }
        !matches!(key_from_keysym(raw_keysym), Key::Unknown | Key::F(_))
    }

    fn is_raw_only_key(key: Key) -> bool {
        matches!(
            key,
            Key::Shift | Key::Control | Key::Alt | Key::Super | Key::CapsLock | Key::NumLock
        )
    }

    fn modifier_key_from_keysym(raw_keysym: u32) -> Option<Key> {
        match raw_keysym {
            key::Shift_L | key::Shift_R => Some(Key::Shift),
            key::Control_L | key::Control_R => Some(Key::Control),
            key::Alt_L | key::Alt_R | key::Meta_L | key::Meta_R => Some(Key::Alt),
            key::Super_L | key::Super_R | key::Hyper_L | key::Hyper_R => Some(Key::Super),
            _ => None,
        }
    }

    fn modifier_key_for_keycode(keymap: &xkb::Keymap, keycode: u8) -> Option<Key> {
        let keycode = xkb::Keycode::new(keycode.into());
        for layout in 0..keymap.num_layouts_for_key(keycode) {
            for level in 0..keymap.num_levels_for_key(keycode, layout) {
                for keysym in keymap.key_get_syms_by_level(keycode, layout, level) {
                    if let Some(key) = modifier_key_from_keysym(keysym.raw()) {
                        return Some(key);
                    }
                }
            }
        }
        None
    }

    fn modifier_keycodes(keymap: &xkb::Keymap, target: Key) -> Vec<u8> {
        let min = keymap.min_keycode().raw();
        let max = keymap.max_keycode().raw();
        (min..=max)
            .filter_map(|raw| u8::try_from(raw).ok())
            .filter(|keycode| modifier_key_for_keycode(keymap, *keycode) == Some(target))
            .collect()
    }

    fn keycode_is_pressed(keys: &[u8; 32], keycode: u8) -> bool {
        let byte = usize::from(keycode / 8);
        let bit = keycode % 8;
        keys[byte] & (1_u8 << bit) != 0
    }

    fn remember_pressed_keycode(pressed: &mut Vec<u8>, keycode: u8) {
        if !pressed.contains(&keycode) {
            pressed.push(keycode);
        }
    }

    fn forget_pressed_keycode(pressed: &mut Vec<u8>, keycode: u8) {
        pressed.retain(|pressed_keycode| *pressed_keycode != keycode);
    }

    fn remember_pressed_shift(pressed: &mut Vec<(u16, u8)>, device_id: u16, keycode: u8) {
        if !pressed.contains(&(device_id, keycode)) {
            pressed.push((device_id, keycode));
        }
    }

    fn forget_pressed_shift(pressed: &mut Vec<(u16, u8)>, device_id: u16, keycode: u8) {
        pressed.retain(|pressed_key| *pressed_key != (device_id, keycode));
    }

    fn pointer_click_boundary(button: u32) -> Option<KeyEvent> {
        matches!(button, 1..=3).then(|| KeyEvent::press(Key::Unknown))
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

    fn prefer_unshifted_keysym(
        resolved_keysym: u32,
        unshifted_keysym: Option<u32>,
        shift_held: bool,
        caps_lock: bool,
    ) -> u32 {
        if shift_held || caps_lock {
            resolved_keysym
        } else {
            unshifted_keysym.unwrap_or(resolved_keysym)
        }
    }

    fn stale_modifier_mask(
        modifier_index: u32,
        modifiers: u32,
        physically_held: bool,
    ) -> Option<u8> {
        if physically_held || modifier_index >= u8::BITS {
            return None;
        }
        let mask = 1_u8 << modifier_index;
        (modifiers & u32::from(mask) != 0).then_some(mask)
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

    #[derive(Clone, Copy)]
    enum SyntheticKey {
        Direct(u8),
        Premapped { keycode: u8, needs_shift: bool },
        Mapped(u32),
    }

    fn queue_fake_key_on(connection: &XCBConnection, keycode: u8) -> Result<()> {
        for event_type in [xproto::KEY_PRESS_EVENT, xproto::KEY_RELEASE_EVENT] {
            let _cookie = connection
                .xtest_fake_input(event_type, keycode, CURRENT_TIME, NONE, 0, 0, 0)
                .map_err(|error| KeyboardError::X11Protocol(error.to_string()))?;
        }
        Ok(())
    }

    fn take_queued_synthetic_key(queue: &mut VecDeque<u8>, detail: u32) -> bool {
        let Ok(keycode) = u8::try_from(detail) else {
            return false;
        };
        if queue.front() == Some(&keycode) {
            queue.pop_front();
            return true;
        }
        if let Some(index) = queue.iter().position(|queued| *queued == keycode) {
            queue.remove(index);
            return true;
        }
        false
    }

    fn synthetic_event_is_identified(
        xtest_device_ids: &[u16],
        premapped: Option<&VietnamesePremappedTable>,
        device_id: u16,
        source_id: u16,
        detail: u32,
        injection_keycode: u8,
    ) -> bool {
        xtest_device_ids.contains(&device_id)
            || xtest_device_ids.contains(&source_id)
            || detail == u32::from(injection_keycode)
            || premapped.is_some_and(|table| {
                u8::try_from(detail).is_ok_and(|kc| table.is_premapped_keycode(kc))
            })
    }

    fn device_name_is_xtest(name: &[u8]) -> bool {
        String::from_utf8_lossy(name)
            .to_ascii_lowercase()
            .contains("xtest")
    }

    fn should_grab_keyboard_device(device_type: DeviceType, enabled: bool, name: &[u8]) -> bool {
        enabled && device_type == DeviceType::SLAVE_KEYBOARD && !device_name_is_xtest(name)
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

    fn ensure_us_keyboard_layout() {
        if let Ok(output) = std::process::Command::new("setxkbmap")
            .arg("-query")
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("layout:     vn")
                || (stdout.contains("vn") && stdout.contains("variant:    us"))
            {
                warn!(
                    "Detected Vietnamese XKB layout (vn). Resetting to standard US layout to prevent dead key issues."
                );
                let _ = std::process::Command::new("setxkbmap").arg("us").output();
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn raw_modifiers_replace_latched_xkb_modifier_flags() {
            let mut raw = RawModifierState::default();
            let mut modifiers = Modifiers {
                ctrl: true,
                shift: true,
                alt: true,
                super_key: true,
                caps_lock: true,
                num_lock: true,
            };

            raw.apply_to(&mut modifiers);
            assert_eq!(
                modifiers,
                Modifiers {
                    caps_lock: true,
                    num_lock: true,
                    ..Modifiers::default()
                }
            );

            raw.press(37, Key::Control);
            raw.press(50, Key::Shift);
            raw.apply_to(&mut modifiers);
            assert!(modifiers.ctrl);
            assert!(modifiers.shift);

            raw.release(50, Key::Shift);
            raw.release(37, Key::Control);
            raw.apply_to(&mut modifiers);
            assert!(!modifiers.ctrl);
            assert!(!modifiers.shift);
        }

        #[test]
        fn pointer_clicks_clear_composition_but_wheel_events_do_not() {
            for button in 1..=3 {
                assert_eq!(
                    pointer_click_boundary(button),
                    Some(KeyEvent::press(Key::Unknown))
                );
            }
            assert_eq!(pointer_click_boundary(4), None);
            assert_eq!(pointer_click_boundary(5), None);
        }

        #[test]
        fn physical_key_is_not_ignored_when_it_matches_queued_xtest_keycode() {
            let xtest_devices = [4, 5];
            let physical_device = 3;
            let physical_source = 14;
            let letter_a_keycode = 38;

            assert!(!synthetic_event_is_identified(
                &xtest_devices,
                None,
                physical_device,
                physical_source,
                letter_a_keycode,
                248,
            ));
            assert!(synthetic_event_is_identified(
                &xtest_devices,
                None,
                physical_device,
                5,
                letter_a_keycode,
                248,
            ));
            assert!(synthetic_event_is_identified(
                &xtest_devices,
                None,
                physical_device,
                physical_source,
                248,
                248,
            ));
        }

        #[test]
        fn shiftless_raw_keys_prefer_the_unshifted_keysym() {
            assert_eq!(
                prefer_unshifted_keysym(key::A, Some(key::a), false, false),
                key::a
            );
            assert_eq!(
                prefer_unshifted_keysym(key::exclam, Some(key::_1), false, false),
                key::_1
            );
            assert_eq!(
                prefer_unshifted_keysym(key::A, Some(key::a), true, false),
                key::A
            );
            assert_eq!(
                prefer_unshifted_keysym(key::A, Some(key::a), false, true),
                key::A
            );
            assert_eq!(prefer_unshifted_keysym(key::A, None, false, false), key::A);
        }

        #[test]
        fn only_stale_physical_modifier_latches_are_cleared() {
            assert_eq!(stale_modifier_mask(0, 0b0001, false), Some(1));
            assert_eq!(stale_modifier_mask(0, 0b0001, true), None);
            assert_eq!(stale_modifier_mask(0, 0, false), None);
            assert_eq!(stale_modifier_mask(8, 0x100, false), None);
        }

        #[test]
        fn physical_key_bitmap_uses_x11_keycode_bits() {
            let mut keys = [0_u8; 32];
            keys[6] = 0b0100_0000;
            assert!(keycode_is_pressed(&keys, 54));
            assert!(!keycode_is_pressed(&keys, 50));
        }

        #[test]
        fn grabs_physical_keyboards_but_never_xtest_devices() {
            assert!(should_grab_keyboard_device(
                DeviceType::SLAVE_KEYBOARD,
                true,
                b"AT Translated Set 2 keyboard",
            ));
            assert!(!should_grab_keyboard_device(
                DeviceType::SLAVE_KEYBOARD,
                true,
                b"Virtual core XTEST keyboard",
            ));
            assert!(!should_grab_keyboard_device(
                DeviceType::SLAVE_KEYBOARD,
                false,
                b"Disconnected keyboard",
            ));
            assert!(!should_grab_keyboard_device(
                DeviceType::SLAVE_POINTER,
                true,
                b"Mouse",
            ));
        }

        #[test]
        fn vietnamese_char_tables_are_consistent_and_complete() {
            assert_eq!(VIETNAMESE_CHAR_PAIRS.len(), 67);
            assert_eq!(POPULAR_UPPERCASE.len(), 67);

            let mut lower_set = std::collections::HashSet::new();
            let mut upper_set = std::collections::HashSet::new();
            for &(lower, upper) in VIETNAMESE_CHAR_PAIRS {
                assert!(lower_set.insert(lower), "duplicate lowercase char: {lower}");
                assert!(upper_set.insert(upper), "duplicate uppercase char: {upper}");
                assert!(Keysym::from_char(lower).raw() != 0);
                assert!(Keysym::from_char(upper).raw() != 0);
            }

            for &upper in POPULAR_UPPERCASE {
                assert!(
                    upper_set.contains(&upper),
                    "missing uppercase char: {upper}"
                );
            }
        }

        #[test]
        fn vital_keys_are_protected_from_borrowing() {
            // Standard typing keys must be vital
            assert!(is_vital_key(9)); // Esc
            assert!(is_vital_key(22)); // BackSpace
            assert!(is_vital_key(36)); // Return
            assert!(is_vital_key(38)); // a
            assert!(is_vital_key(50)); // Shift_L
            assert!(is_vital_key(64)); // Alt_L
            assert!(is_vital_key(65)); // Space
            assert!(is_vital_key(111)); // Up
            assert!(is_vital_key(116)); // Down
            assert!(is_vital_key(121)); // AudioMute
            assert!(is_vital_key(133)); // Super_L
            assert!(is_vital_key(204)); // Alt_L
            assert!(is_vital_key(232)); // MonBrightnessDown

            // Safe keys must NOT be vital
            assert!(!is_vital_key(93));
            assert!(!is_vital_key(98)); // Katakana
            assert!(!is_vital_key(136)); // Cancel
            assert!(!is_vital_key(150)); // Sleep
            assert!(!is_vital_key(180)); // HomePage
            assert!(!is_vital_key(210)); // Launch3
            assert!(!is_vital_key(240)); // Reply
            assert!(!is_vital_key(250)); // Prev_VMode
        }
    }
}

pub use platform::X11KeyboardBackend;
