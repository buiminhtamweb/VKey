use std::process::ExitCode;

use keyboard_linux::{KeyboardBackend, KeyboardDecision, X11KeyboardBackend};
use tracing_subscriber::EnvFilter;
use vietnamese_core::{Key, KeyState, Modifiers};

fn main() -> ExitCode {
    if let Err(error) = init_tracing() {
        eprintln!("Failed to initialize logging: {error}");
        return ExitCode::FAILURE;
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("keyboard-debug: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> keyboard_linux::Result<()> {
    let mut backend = X11KeyboardBackend::new()?;
    backend.start()?;
    println!("X11 keyboard backend started.\n\nPress keys (Escape exits)...");

    loop {
        let event = backend.next_event()?;
        println!(
            "{} {:?} modifiers={}",
            state_name(event.state),
            event.key,
            format_modifiers(event.modifiers)
        );
        backend.decide(KeyboardDecision::PassThrough)?;
        if event.state == KeyState::Press && event.key == Key::Escape {
            break;
        }
    }
    backend.stop()
}

fn init_tracing() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).try_init()
}

fn state_name(state: KeyState) -> &'static str {
    match state {
        KeyState::Press => "KeyPress",
        KeyState::Release => "KeyRelease",
    }
}

fn format_modifiers(modifiers: Modifiers) -> String {
    let names = [
        (modifiers.shift, "SHIFT"),
        (modifiers.ctrl, "CTRL"),
        (modifiers.alt, "ALT"),
        (modifiers.super_key, "SUPER"),
        (modifiers.caps_lock, "CAPS_LOCK"),
        (modifiers.num_lock, "NUM_LOCK"),
    ];
    let active = names
        .into_iter()
        .filter_map(|(active, name)| active.then_some(name))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{active}]")
}
