#![cfg(target_os = "linux")]

use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    mem,
    os::fd::AsRawFd,
    slice,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use keyboard_linux::{
    KeyboardBackend, KeyboardDecision, X11KeyboardBackend, decision_for, execute_engine_action,
};
use unicode_segmentation::UnicodeSegmentation;
use vietnamese_core::{EngineConfig, InputEngine};
use x11rb::{
    CURRENT_TIME,
    connection::Connection,
    protocol::{
        Event,
        xinput::{ConnectionExt as _, Device},
        xproto::{
            ConnectionExt as _, CreateWindowAux, EventMask, InputFocus, KeyButMask, MapState,
            WindowClass,
        },
    },
    wrapper::ConnectionExt as _,
    xcb_ffi::XCBConnection,
};
use xkeysym::{Keysym, key};

const DEVICE_NAME: &str = "VKey X11 stress keyboard";
const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const SYN_REPORT: u16 = 0;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_RIGHTSHIFT: u16 = 54;
const KEY_SPACE: u16 = 57;

const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_WRITE: u32 = 1;

const fn ioctl_none(kind: u8, number: u8) -> libc::c_ulong {
    ((kind as u32) << IOC_TYPESHIFT | (number as u32) << IOC_NRSHIFT) as libc::c_ulong
}

const fn ioctl_write<T>(kind: u8, number: u8) -> libc::c_ulong {
    ((IOC_WRITE << IOC_DIRSHIFT)
        | ((kind as u32) << IOC_TYPESHIFT)
        | ((number as u32) << IOC_NRSHIFT)
        | ((mem::size_of::<T>() as u32) << IOC_SIZESHIFT)) as libc::c_ulong
}

const UI_SET_EVBIT: libc::c_ulong = ioctl_write::<libc::c_int>(b'U', 100);
const UI_SET_KEYBIT: libc::c_ulong = ioctl_write::<libc::c_int>(b'U', 101);
const UI_DEV_CREATE: libc::c_ulong = ioctl_none(b'U', 1);
const UI_DEV_DESTROY: libc::c_ulong = ioctl_none(b'U', 2);
const UI_DEV_SETUP: libc::c_ulong = ioctl_write::<libc::uinput_setup>(b'U', 3);

#[test]
#[ignore = "requires X11 and write access to /dev/uinput"]
fn sustained_mixed_case_vietnamese_input_never_leaks_shift() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("keyboard_linux=debug")),
        )
        .try_init();
    assert_eq!(
        std::env::var("VKEY_X11_STRESS_TEST").as_deref(),
        Ok("1"),
        "set VKEY_X11_STRESS_TEST=1 to run the X11/uinput stress test"
    );

    let mut keyboard = VirtualKeyboard::new().expect("create uinput keyboard");
    wait_for_x11_device(DEVICE_NAME).expect("wait for X11 keyboard hotplug");

    let target = X11TextTarget::new().expect("create focused X11 text target");
    let captured = target.text.clone();
    let target_stop = target.stop.clone();
    let (target_thread, target_ready) = target.spawn_reader();
    target_ready
        .recv_timeout(Duration::from_secs(2))
        .expect("start X11 target reader");

    let backend_stop = Arc::new(AtomicBool::new(false));
    let backend_stop_clone = backend_stop.clone();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let backend_thread = thread::spawn(move || -> Result<(), String> {
        let mut backend = X11KeyboardBackend::new().map_err(|error| error.to_string())?;
        backend.start().map_err(|error| error.to_string())?;
        ready_tx.send(()).map_err(|error| error.to_string())?;
        let mut engine = InputEngine::new(EngineConfig::default());

        while !backend_stop_clone.load(Ordering::Acquire) {
            let event = match backend.next_event() {
                Ok(event) => event,
                Err(keyboard_linux::KeyboardError::Timeout) => continue,
                Err(error) => return Err(error.to_string()),
            };
            let action = engine.process_key(event);
            let decision = decision_for(&action);
            backend
                .decide(decision)
                .map_err(|error| error.to_string())?;
            if decision == KeyboardDecision::Consume {
                let mut injector = backend.text_injector();
                execute_engine_action(&mut injector, &action).map_err(|error| error.to_string())?;
            }
        }

        backend.stop().map_err(|error| error.to_string())
    });

    ready_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("start X11 keyboard backend");
    thread::sleep(Duration::from_millis(50));

    let repetitions = std::env::var("VKEY_X11_STRESS_REPETITIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(100);
    let expected = "Bùi Minh Tâm ".repeat(repetitions);
    let expected_graphemes = expected.graphemes(true).count();
    for iteration in 0..repetitions {
        keyboard
            .type_ascii("Buif Minh Taam ", iteration % 2 == 1, iteration % 4 < 2)
            .expect("emit uinput phrase");
        if iteration % 5 == 4 {
            thread::yield_now();
        }
    }

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let current_graphemes = captured.lock().unwrap().graphemes(true).count();
        if current_graphemes >= expected_graphemes || Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    backend_stop.store(true, Ordering::Release);
    backend_thread
        .join()
        .expect("join keyboard backend")
        .expect("run keyboard backend");
    target_stop.store(true, Ordering::Release);
    target_thread.join().expect("join X11 target");

    assert_eq!(*captured.lock().unwrap(), expected);
}

struct VirtualKeyboard {
    file: File,
}

impl VirtualKeyboard {
    fn new() -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/uinput")?;
        let fd = file.as_raw_fd();
        ioctl_value(fd, UI_SET_EVBIT, EV_KEY)?;
        for code in supported_keycodes() {
            ioctl_value(fd, UI_SET_KEYBIT, code)?;
        }

        let mut setup: libc::uinput_setup = unsafe { mem::zeroed() };
        setup.id.bustype = 0x03;
        setup.id.vendor = 0x1d6b;
        setup.id.product = 0x0104;
        setup.id.version = 1;
        for (destination, source) in setup.name.iter_mut().zip(DEVICE_NAME.bytes()) {
            *destination = source as libc::c_char;
        }
        ioctl_pointer(fd, UI_DEV_SETUP, &setup)?;
        ioctl_no_arg(fd, UI_DEV_CREATE)?;

        Ok(Self { file })
    }

    fn type_ascii(
        &mut self,
        text: &str,
        right_shift: bool,
        release_shift_before_letter: bool,
    ) -> io::Result<()> {
        let shift = if right_shift {
            KEY_RIGHTSHIFT
        } else {
            KEY_LEFTSHIFT
        };
        for character in text.chars() {
            let uppercase = character.is_ascii_uppercase();
            let code = evdev_keycode(character.to_ascii_lowercase()).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unsupported test character {character:?}"),
                )
            })?;
            if uppercase {
                self.emit_key(shift, 1)?;
            }
            self.emit_key(code, 1)?;
            if uppercase && release_shift_before_letter {
                self.emit_key(shift, 0)?;
            }
            self.emit_key(code, 0)?;
            if uppercase && !release_shift_before_letter {
                self.emit_key(shift, 0)?;
            }
        }
        Ok(())
    }

    fn emit_key(&mut self, code: u16, value: i32) -> io::Result<()> {
        self.emit(EV_KEY, code, value)?;
        self.emit(EV_SYN, SYN_REPORT, 0)?;
        thread::sleep(Duration::from_micros(500));
        Ok(())
    }

    fn emit(&mut self, event_type: u16, code: u16, value: i32) -> io::Result<()> {
        let event = libc::input_event {
            time: unsafe { mem::zeroed() },
            type_: event_type,
            code,
            value,
        };
        // SAFETY: input_event is a plain C structure and remains alive for the
        // complete write of its byte representation.
        let bytes = unsafe {
            slice::from_raw_parts(
                (&event as *const libc::input_event).cast::<u8>(),
                mem::size_of::<libc::input_event>(),
            )
        };
        self.file.write_all(bytes)
    }
}

impl Drop for VirtualKeyboard {
    fn drop(&mut self) {
        let _ = self.emit_key(KEY_LEFTSHIFT, 0);
        let _ = self.emit_key(KEY_RIGHTSHIFT, 0);
        let _ = ioctl_no_arg(self.file.as_raw_fd(), UI_DEV_DESTROY);
    }
}

struct X11TextTarget {
    connection: XCBConnection,
    window: u32,
    previous_focus: u32,
    text: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
}

impl X11TextTarget {
    fn new() -> Result<Self, String> {
        let (connection, screen_number) =
            XCBConnection::connect(None).map_err(|error| error.to_string())?;
        let screen = &connection.setup().roots[screen_number];
        let window = connection
            .generate_id()
            .map_err(|error| error.to_string())?;
        let previous_focus = connection
            .get_input_focus()
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?
            .focus;
        connection
            .create_window(
                screen.root_depth,
                window,
                screen.root,
                0,
                0,
                8,
                8,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new().event_mask(EventMask::KEY_PRESS),
            )
            .map_err(|error| error.to_string())?
            .check()
            .map_err(|error| error.to_string())?;
        connection
            .map_window(window)
            .map_err(|error| error.to_string())?
            .check()
            .map_err(|error| error.to_string())?;
        connection.sync().map_err(|error| error.to_string())?;
        let map_deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let attributes = connection
                .get_window_attributes(window)
                .map_err(|error| error.to_string())?
                .reply()
                .map_err(|error| error.to_string())?;
            if attributes.map_state == MapState::VIEWABLE {
                break;
            }
            if Instant::now() >= map_deadline {
                return Err("X11 text target did not become viewable".to_owned());
            }
            thread::sleep(Duration::from_millis(10));
        }
        connection
            .set_input_focus(InputFocus::POINTER_ROOT, window, CURRENT_TIME)
            .map_err(|error| error.to_string())?
            .check()
            .map_err(|error| error.to_string())?;
        connection.sync().map_err(|error| error.to_string())?;

        Ok(Self {
            connection,
            window,
            previous_focus,
            text: Arc::new(Mutex::new(String::new())),
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    fn spawn_reader(self) -> (thread::JoinHandle<()>, mpsc::Receiver<()>) {
        let text = self.text.clone();
        let stop = self.stop.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let handle = thread::spawn(move || {
            let mut mapping =
                read_keyboard_mapping(&self.connection).expect("read initial X11 keyboard mapping");
            ready_tx.send(()).expect("signal X11 target readiness");
            while !stop.load(Ordering::Acquire) {
                match self.connection.poll_for_event() {
                    Ok(Some(Event::KeyPress(event))) => {
                        if let Some(keysym) = keysym_for_event(&mapping, &event) {
                            apply_keysym(&mut text.lock().unwrap(), keysym);
                        }
                    }
                    Ok(Some(Event::MappingNotify(_))) => {
                        mapping = read_keyboard_mapping(&self.connection)
                            .expect("refresh X11 keyboard mapping");
                    }
                    Ok(Some(_)) | Ok(None) => thread::sleep(Duration::from_micros(100)),
                    Err(error) => panic!("read X11 target event: {error}"),
                }
            }
            let _ = self.connection.set_input_focus(
                InputFocus::POINTER_ROOT,
                self.previous_focus,
                CURRENT_TIME,
            );
            let _ = self.connection.destroy_window(self.window);
            let _ = self.connection.flush();
        });
        (handle, ready_rx)
    }
}

struct KeyboardMapping {
    min_keycode: u8,
    width: usize,
    keysyms: Vec<u32>,
}

fn read_keyboard_mapping(connection: &XCBConnection) -> Result<KeyboardMapping, String> {
    let setup = connection.setup();
    let min_keycode = setup.min_keycode;
    let count = setup
        .max_keycode
        .checked_sub(min_keycode)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| "invalid X11 keycode range".to_owned())?;
    let mapping = connection
        .get_keyboard_mapping(min_keycode, count)
        .map_err(|error| error.to_string())?
        .reply()
        .map_err(|error| error.to_string())?;
    Ok(KeyboardMapping {
        min_keycode,
        width: usize::from(mapping.keysyms_per_keycode),
        keysyms: mapping.keysyms,
    })
}

fn keysym_for_event(
    mapping: &KeyboardMapping,
    event: &x11rb::protocol::xproto::KeyPressEvent,
) -> Option<u32> {
    let shifted = u16::from(event.state) & u16::from(KeyButMask::SHIFT) != 0;
    let level = usize::from(shifted);
    let key_offset = usize::from(event.detail.checked_sub(mapping.min_keycode)?);
    let start = key_offset.checked_mul(mapping.width)?;
    mapping
        .keysyms
        .get(start + level)
        .copied()
        .filter(|keysym| *keysym != 0)
        .or_else(|| mapping.keysyms.get(start).copied())
}

fn apply_keysym(text: &mut String, keysym: u32) {
    if keysym == key::BackSpace {
        if let Some((start, _)) = text.grapheme_indices(true).next_back() {
            text.truncate(start);
        }
        return;
    }
    if let Some(character) = Keysym::new(keysym).key_char() {
        text.push(character);
    }
}

fn wait_for_x11_device(name: &str) -> Result<(), String> {
    let (connection, _) = XCBConnection::connect(None).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let reply = connection
            .xinput_xi_query_device(u16::from(Device::ALL))
            .map_err(|error| error.to_string())?
            .reply()
            .map_err(|error| error.to_string())?;
        if reply
            .infos
            .iter()
            .any(|info| info.name.as_slice() == name.as_bytes())
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(format!("X11 did not attach virtual keyboard {name:?}"))
}

fn supported_keycodes() -> impl Iterator<Item = u16> {
    [
        KEY_LEFTSHIFT,
        KEY_RIGHTSHIFT,
        KEY_SPACE,
        16,
        17,
        18,
        19,
        20,
        21,
        22,
        23,
        24,
        25,
        30,
        31,
        32,
        33,
        34,
        35,
        36,
        37,
        38,
        44,
        45,
        46,
        47,
        48,
        49,
        50,
    ]
    .into_iter()
}

fn evdev_keycode(character: char) -> Option<u16> {
    Some(match character {
        'a' => 30,
        'b' => 48,
        'c' => 46,
        'd' => 32,
        'e' => 18,
        'f' => 33,
        'g' => 34,
        'h' => 35,
        'i' => 23,
        'j' => 36,
        'k' => 37,
        'l' => 38,
        'm' => 50,
        'n' => 49,
        'o' => 24,
        'p' => 25,
        'q' => 16,
        'r' => 19,
        's' => 31,
        't' => 20,
        'u' => 22,
        'v' => 47,
        'w' => 17,
        'x' => 45,
        'y' => 21,
        'z' => 44,
        ' ' => KEY_SPACE,
        _ => return None,
    })
}

fn ioctl_value(fd: libc::c_int, request: libc::c_ulong, value: u16) -> io::Result<()> {
    // SAFETY: uinput bit-setting ioctls take an integer value as their third argument.
    let result = unsafe { libc::ioctl(fd, request, libc::c_int::from(value)) };
    ioctl_result(result)
}

fn ioctl_pointer<T>(fd: libc::c_int, request: libc::c_ulong, value: &T) -> io::Result<()> {
    // SAFETY: value points to the request structure expected by this ioctl and
    // remains valid for the duration of the call.
    let result = unsafe { libc::ioctl(fd, request, value) };
    ioctl_result(result)
}

fn ioctl_no_arg(fd: libc::c_int, request: libc::c_ulong) -> io::Result<()> {
    // SAFETY: these uinput lifecycle ioctls do not take a third argument.
    let result = unsafe { libc::ioctl(fd, request) };
    ioctl_result(result)
}

fn ioctl_result(result: libc::c_int) -> io::Result<()> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
