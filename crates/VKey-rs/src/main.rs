#![windows_subsystem = "windows"]

mod config_store;
mod gui;

use eframe::egui;
use std::sync::mpsc::{self, Receiver, Sender};
use std::{env, process::ExitCode};

#[cfg(not(target_os = "linux"))]
use keyboard_linux::execute_engine_action;
use keyboard_linux::{
    KeyboardBackend, KeyboardDecision, X11KeyboardBackend, decision_for,
    execute_observed_engine_action,
};
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
    backend.start()?;

    // Track modifiers state for quick-toggle hotkey
    let mut ctrl_pressed = false;
    let mut shift_pressed = false;
    let mut alt_pressed = false;

    loop {
        // Poll for configuration/command updates from GUI thread
        while let Ok(msg) = rx.try_recv() {
            match msg {
                AppMessage::UpdateConfig(new_config) => {
                    config = new_config;
                    engine.set_config(config.clone());
                }
                AppMessage::Exit => {
                    let _ = backend.stop();
                    return Ok(());
                }
            }
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

        // Hotkey Toggle Detection: Ctrl+Shift or Alt+Z
        let mut toggle_triggered = false;
        match config.shortcut_key {
            vietnamese_core::config::ShortcutKey::CtrlShift => match event.key {
                vietnamese_core::Key::Control => {
                    let pressed = event.state == vietnamese_core::KeyState::Press;
                    if pressed && !ctrl_pressed && shift_pressed {
                        toggle_triggered = true;
                    }
                    ctrl_pressed = pressed;
                }
                vietnamese_core::Key::Shift => {
                    let pressed = event.state == vietnamese_core::KeyState::Press;
                    if pressed && !shift_pressed && ctrl_pressed {
                        toggle_triggered = true;
                    }
                    shift_pressed = pressed;
                }
                _ => {}
            },
            vietnamese_core::config::ShortcutKey::AltZ => match event.key {
                vietnamese_core::Key::Alt => {
                    let pressed = event.state == vietnamese_core::KeyState::Press;
                    alt_pressed = pressed;
                }
                vietnamese_core::Key::Character('z') | vietnamese_core::Key::Character('Z')
                    if event.state == vietnamese_core::KeyState::Press
                        && (alt_pressed || event.modifiers.alt) =>
                {
                    toggle_triggered = true;
                }
                _ => {}
            },
        }

        if toggle_triggered {
            config.enabled = !config.enabled;
            engine.set_config(config.clone());

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
            // Perform injection FIRST while the device is still frozen by the
            // sync grab.  XTest-injected events bypass passive grabs, so they
            // reach the focused application immediately.  Only after injection
            // completes do we thaw the device (via `decide`), guaranteeing that
            // any fast-typed physical key queued behind the grab cannot overtake
            // the injected characters.
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

            // NOW thaw the device — the injected text is already in the
            // application's event queue, so the next physical key will arrive
            // in the correct order.
            backend.decide(decision)?;
        } else {
            backend.decide(decision)?;
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
