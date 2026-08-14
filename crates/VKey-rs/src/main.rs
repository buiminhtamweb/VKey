#![windows_subsystem = "windows"]

mod config_store;
mod gui;

use eframe::egui;
use std::sync::mpsc::{self, Receiver, Sender};
use std::{env, process::ExitCode};

#[cfg(not(target_os = "linux"))]
use keyboard_linux::execute_engine_action;
#[cfg(target_os = "linux")]
use keyboard_linux::execute_observed_engine_action;
use keyboard_linux::{KeyboardBackend, KeyboardDecision, X11KeyboardBackend, decision_for};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use vietnamese_core::{EngineConfig, InputEngine, InputMethod};

pub enum AppMessage {
    UpdateConfig(EngineConfig),
    Exit,
}

pub enum GuiMessage {
    StateChanged(EngineConfig),
    ShowSettingsWindow,
}

#[derive(Debug, Default)]
struct Options {
    debug_input: bool,
    input_method: Option<InputMethod>,
    disabled: bool,
    headless: bool,
}

#[cfg(target_os = "windows")]
fn show_existing_window() {
    unsafe {
        let title: Vec<u16> = "VKey Settings\0".encode_utf16().collect();
        let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(
            std::ptr::null(),
            title.as_ptr(),
        );
        if hwnd != 0 {
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_RESTORE,
            );
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOW,
            );
            windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn show_existing_window() {}

fn main() -> ExitCode {
    // Ensure only one instance of VKey is running
    let _instance_lock = match std::net::TcpListener::bind("127.0.0.1:58989") {
        Ok(listener) => listener,
        Err(_) => {
            eprintln!("VKey is already running.");
            show_existing_window();
            return ExitCode::SUCCESS;
        }
    };

    let options = match parse_options() {
        Ok(Some(options)) => options,
        Ok(None) => return ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("VKey-rs: {message}\nUse --help for usage.");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = init_tracing() {
        eprintln!("VKey-rs: failed to initialize logging: {error}");
        return ExitCode::FAILURE;
    }

    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("VKey-rs: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load config from file
    let mut config = config_store::load_config();

    // 2. Override config with CLI flags
    if let Some(input_method) = options.input_method {
        config.input_method = input_method;
    }
    if options.disabled {
        config.enabled = false;
    }

    // Save initial overridden config
    config_store::save_config(&config);

    if options.headless {
        info!("Running in headless mode (daemon only)...");
        let (_tx, _rx) = mpsc::channel();
        let (_gui_tx, _gui_rx) = mpsc::channel();
        // Run in main thread directly with a dummy context
        run_daemon(config, _rx, _gui_tx, egui::Context::default())?;
        Ok(())
    } else {
        info!("Starting GUI settings application and background daemon...");
        init_platform_gui()?;

        let (tx, rx) = mpsc::channel();
        let (gui_tx, gui_rx) = mpsc::channel();

        // Run egui settings panel in the main thread
        let native_options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("VKey Settings")
                .with_inner_size([350.0, 440.0])
                .with_resizable(false)
                .with_maximize_button(false),
            ..Default::default()
        };

        eframe::run_native(
            "VKey Settings",
            native_options,
            Box::new(move |cc| {
                let ctx = cc.egui_ctx.clone();
                gui::setup_custom_fonts(&ctx);

                let mut visual = egui::Visuals::light();
                visual.window_fill = egui::Color32::from_rgb(245, 247, 250);
                visual.panel_fill = egui::Color32::from_rgb(245, 247, 250);
                visual.override_text_color = Some(egui::Color32::BLACK);
                ctx.set_visuals(visual);

                // Spawn background keyboard daemon thread inside the main loop closure
                let daemon_config = config.clone();
                let ctx_clone = ctx.clone();
                let gui_tx_clone = gui_tx.clone();
                std::thread::spawn(move || {
                    if let Err(error) = run_daemon(daemon_config, rx, gui_tx_clone, ctx_clone) {
                        error!("Daemon background thread error: {}", error);
                    }
                });

                Ok(Box::new(gui::AppGui::new(config, tx, gui_rx, gui_tx, ctx)))
            }),
        )
        .map_err(|e| e.to_string().into())
    }
}

#[cfg(target_os = "linux")]
fn init_platform_gui() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn init_platform_gui() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

fn run_daemon(
    initial_config: EngineConfig,
    rx: Receiver<AppMessage>,
    gui_tx: Sender<GuiMessage>,
    ctx: egui::Context,
) -> keyboard_linux::Result<()> {
    let mut config = initial_config;
    let mut engine = InputEngine::new(config.clone());
    let mut backend = X11KeyboardBackend::new()?;
    let mut shortcut_state = ShortcutState::default();
    backend.start()?;

    loop {
        if apply_daemon_messages(&rx, &mut config, &mut engine, &mut shortcut_state, &gui_tx) {
            let _ = backend.stop();
            return Ok(());
        }

        let event = match backend.next_event() {
            Ok(ev) => ev,
            Err(keyboard_linux::KeyboardError::Timeout) => {
                continue;
            }
            Err(e) => {
                let _ = backend.stop();
                return Err(e);
            }
        };

        // A GUI update can arrive while the backend is blocked waiting for the
        // next X11 event. Apply it before interpreting that event so a stale
        // configuration cannot overwrite a just-triggered shortcut toggle.
        if apply_daemon_messages(&rx, &mut config, &mut engine, &mut shortcut_state, &gui_tx) {
            let _ = backend.stop();
            return Ok(());
        }

        let toggle_triggered = shortcut_state.update(config.shortcut_key, event);

        if toggle_triggered {
            config.enabled = !config.enabled;
            engine.set_config(config.clone());
            info!(
                enabled = config.enabled,
                shortcut = ?config.shortcut_key,
                "Keyboard shortcut toggled Vietnamese input"
            );

            // Save toggled state to config storage
            config_store::save_config(&config);

            // Send notification back to GUI thread to redraw tray icon
            let _ = gui_tx.send(GuiMessage::StateChanged(config.clone()));

            // Wake up GUI thread immediately
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }

        let action = engine.process_key(event);
        let decision = decision_for(&action);

        if decision == KeyboardDecision::Consume {
            // A Windows low-level hook must receive the consume decision before
            // SendInput runs. Otherwise the hook can hit its decision timeout,
            // pass the original Telex/VNI key to the application, and only then
            // apply the replacement (for example `buif` becoming `buùi`).
            #[cfg(target_os = "windows")]
            backend.decide(decision)?;

            // On X11, the target already receives the physical key. Repair the
            // observed text by deleting the raw grapheme(s) and queueing the
            // replacement on the same X11 connection.
            #[cfg(target_os = "linux")]
            let observed_inserted_graphemes = observed_inserted_graphemes(event);
            #[cfg(target_os = "linux")]
            let observed_deleted_graphemes = observed_deleted_graphemes(event);
            let result = {
                let mut injector = backend.text_injector();
                #[cfg(target_os = "linux")]
                {
                    execute_observed_engine_action(
                        &mut injector,
                        &action,
                        observed_inserted_graphemes,
                        observed_deleted_graphemes,
                    )
                }
                #[cfg(not(target_os = "linux"))]
                {
                    execute_engine_action(&mut injector, &action)
                }
            };
            if let Err(injection_error) = result {
                engine.reset();
                error!(error = %injection_error, "Text injection failed; composition reset");
            }

            // Non-Windows backends do not use the Windows hook handshake.
            #[cfg(not(target_os = "windows"))]
            backend.decide(decision)?;
        } else {
            backend.decide(decision)?;
        }
    }
}

fn apply_daemon_messages(
    rx: &Receiver<AppMessage>,
    config: &mut EngineConfig,
    engine: &mut InputEngine,
    shortcut_state: &mut ShortcutState,
    gui_tx: &Sender<GuiMessage>,
) -> bool {
    while let Ok(msg) = rx.try_recv() {
        match msg {
            AppMessage::UpdateConfig(new_config) => {
                if new_config.shortcut_key != config.shortcut_key {
                    shortcut_state.reset();
                }
                *config = new_config;
                engine.set_config(config.clone());

                // The daemon owns the active configuration. Acknowledge every
                // update so the settings window and Linux tray can converge on
                // this exact state without sending it back to the daemon.
                let _ = gui_tx.send(GuiMessage::StateChanged(config.clone()));
            }
            AppMessage::Exit => return true,
        }
    }

    false
}

#[derive(Debug, Default)]
struct ShortcutState {
    ctrl: bool,
    shift: bool,
    alt: bool,
    ctrl_shift_active: bool,
}

impl ShortcutState {
    fn update(
        &mut self,
        shortcut: vietnamese_core::config::ShortcutKey,
        event: vietnamese_core::KeyEvent,
    ) -> bool {
        self.apply_modifier_event(event);

        // Ctrl+Shift is made only from explicit physical modifier events.
        // Linux desktop layout switching can leave the XKB modifier snapshot
        // latched even after both keys have been released.
        let ctrl = self.ctrl;
        let shift = self.shift;
        let alt = self.alt || event.modifiers.alt;
        let ctrl_shift_active = ctrl && shift;

        let triggered = match shortcut {
            vietnamese_core::config::ShortcutKey::CtrlShift => {
                event.state == vietnamese_core::KeyState::Press
                    && ctrl_shift_active
                    && !self.ctrl_shift_active
            }
            vietnamese_core::config::ShortcutKey::AltZ => {
                event.state == vietnamese_core::KeyState::Press
                    && matches!(
                        event.key,
                        vietnamese_core::Key::Character('z') | vietnamese_core::Key::Character('Z')
                    )
                    && alt
            }
        };

        self.ctrl_shift_active = ctrl_shift_active;
        triggered
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn apply_modifier_event(&mut self, event: vietnamese_core::KeyEvent) {
        let pressed = event.state == vietnamese_core::KeyState::Press;
        match event.key {
            vietnamese_core::Key::Control => self.ctrl = pressed,
            vietnamese_core::Key::Shift => self.shift = pressed,
            vietnamese_core::Key::Alt => self.alt = pressed,
            _ => {}
        }
    }
}

#[cfg(target_os = "linux")]
fn observed_inserted_graphemes(event: vietnamese_core::KeyEvent) -> usize {
    matches!(event.key, vietnamese_core::Key::Character(_)).into()
}

#[cfg(target_os = "linux")]
fn observed_deleted_graphemes(event: vietnamese_core::KeyEvent) -> usize {
    matches!(event.key, vietnamese_core::Key::Backspace).into()
}

fn parse_options() -> Result<Option<Options>, String> {
    let mut options = Options::default();
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--debug-input" => options.debug_input = true,
            "--vni" => options.input_method = Some(InputMethod::Vni),
            "--telex" => options.input_method = Some(InputMethod::Telex),
            "--disabled" => options.disabled = true,
            "--headless" => options.headless = true,
            "-h" | "--help" => {
                println!(
                    "VKey-rs Phase 5\n\n\
                     Usage: VKey-rs [--debug-input] [--telex|--vni] [--disabled] [--headless]\n\n\
                     Runs the background X11 input loop and native egui settings dashboard."
                );
                return Ok(None);
            }
            unknown => return Err(format!("unknown option: {unknown}")),
        }
    }
    Ok(Some(options))
}

fn init_tracing() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).try_init()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vietnamese_core::{Key, KeyEvent, KeyState, Modifiers, ShortcutKey};

    fn event(key: Key, modifiers: Modifiers, state: KeyState) -> KeyEvent {
        KeyEvent {
            key,
            modifiers,
            state,
        }
    }

    fn press(key: Key, modifiers: Modifiers) -> KeyEvent {
        event(key, modifiers, KeyState::Press)
    }

    fn release(key: Key, modifiers: Modifiers) -> KeyEvent {
        event(key, modifiers, KeyState::Release)
    }

    #[test]
    fn ctrl_shift_shortcut_triggers_when_shift_arrives_second() {
        let mut shortcuts = ShortcutState::default();

        assert!(!shortcuts.update(
            ShortcutKey::CtrlShift,
            press(Key::Control, Modifiers::default())
        ));
        assert!(shortcuts.update(
            ShortcutKey::CtrlShift,
            press(Key::Shift, Modifiers::default())
        ));
        assert!(!shortcuts.update(
            ShortcutKey::CtrlShift,
            press(
                Key::Character('x'),
                Modifiers {
                    ctrl: true,
                    shift: true,
                    ..Modifiers::default()
                }
            )
        ));
    }

    #[test]
    fn ctrl_shift_shortcut_triggers_when_control_arrives_second() {
        let mut shortcuts = ShortcutState::default();

        assert!(!shortcuts.update(
            ShortcutKey::CtrlShift,
            press(Key::Shift, Modifiers::default())
        ));
        assert!(shortcuts.update(
            ShortcutKey::CtrlShift,
            press(Key::Control, Modifiers::default())
        ));
    }

    #[test]
    fn ctrl_shift_shortcut_resets_after_modifier_release() {
        let mut shortcuts = ShortcutState::default();

        assert!(!shortcuts.update(
            ShortcutKey::CtrlShift,
            press(Key::Control, Modifiers::default())
        ));
        assert!(shortcuts.update(
            ShortcutKey::CtrlShift,
            press(Key::Shift, Modifiers::default())
        ));
        assert!(!shortcuts.update(
            ShortcutKey::CtrlShift,
            release(Key::Shift, Modifiers::default())
        ));
        assert!(shortcuts.update(
            ShortcutKey::CtrlShift,
            press(Key::Shift, Modifiers::default())
        ));
    }

    #[test]
    fn ctrl_shift_ignores_latched_xkb_modifiers_after_release() {
        let mut shortcuts = ShortcutState::default();
        let latched = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        };

        assert!(!shortcuts.update(ShortcutKey::CtrlShift, press(Key::Control, latched)));
        assert!(shortcuts.update(ShortcutKey::CtrlShift, press(Key::Shift, latched)));
        assert!(!shortcuts.update(ShortcutKey::CtrlShift, release(Key::Shift, latched)));
        assert!(!shortcuts.update(ShortcutKey::CtrlShift, release(Key::Control, latched)));

        assert!(!shortcuts.update(ShortcutKey::CtrlShift, press(Key::Shift, latched)));
        assert!(shortcuts.update(ShortcutKey::CtrlShift, press(Key::Control, latched)));
    }

    #[test]
    fn daemon_config_update_is_acknowledged_without_changing_it() {
        let (app_tx, app_rx) = mpsc::channel();
        let (gui_tx, gui_rx) = mpsc::channel();
        let mut config = EngineConfig::default();
        let mut engine = InputEngine::new(config.clone());
        let mut shortcuts = ShortcutState::default();
        let mut expected = config.clone();
        expected.enabled = false;
        expected.shortcut_key = ShortcutKey::AltZ;

        app_tx
            .send(AppMessage::UpdateConfig(expected.clone()))
            .unwrap();

        assert!(!apply_daemon_messages(
            &app_rx,
            &mut config,
            &mut engine,
            &mut shortcuts,
            &gui_tx,
        ));
        assert_eq!(config, expected);
        assert_eq!(engine.config(), &expected);
        assert!(matches!(
            gui_rx.recv().unwrap(),
            GuiMessage::StateChanged(received) if received == expected
        ));
    }

    #[test]
    fn alt_z_shortcut_requires_alt_and_accepts_uppercase_z() {
        let mut shortcuts = ShortcutState::default();

        assert!(!shortcuts.update(ShortcutKey::AltZ, press(Key::Alt, Modifiers::default())));
        assert!(shortcuts.update(
            ShortcutKey::AltZ,
            press(Key::Character('z'), Modifiers::default())
        ));
        assert!(!shortcuts.update(ShortcutKey::AltZ, release(Key::Alt, Modifiers::default())));
        assert!(shortcuts.update(
            ShortcutKey::AltZ,
            press(
                Key::Character('Z'),
                Modifiers {
                    alt: true,
                    shift: true,
                    ..Modifiers::default()
                }
            )
        ));
        assert!(!ShortcutState::default().update(
            ShortcutKey::AltZ,
            press(Key::Character('z'), Modifiers::default())
        ));
    }
}
