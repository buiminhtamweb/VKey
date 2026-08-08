use std::process::ExitCode;

use keyboard_linux::{KeyboardBackend, KeyboardDecision, X11KeyboardBackend};
use tracing_subscriber::EnvFilter;
use vietnamese_core::{EngineConfig, InputEngine, Key, KeyState};

fn main() -> ExitCode {
    if let Err(error) = init_tracing() {
        eprintln!("Failed to initialize logging: {error}");
        return ExitCode::FAILURE;
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("keyboard-core-debug: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> keyboard_linux::Result<()> {
    let mut backend = X11KeyboardBackend::new()?;
    let mut engine = InputEngine::new(EngineConfig::default());
    backend.start()?;
    println!(
        "X11 -> Vietnamese core debug started.\nPress keys (Escape exits); no text will be injected.\n"
    );

    loop {
        let event = backend.next_event()?;
        let should_exit = event.state == KeyState::Press && event.key == Key::Escape;
        let action = engine.process_key(event);
        let decision = KeyboardDecision::from_engine_action(&action);
        println!(
            "Key: {:?} {:?}\nEngine: {:?}\nDecision: {:?}\nResult: {}\n",
            event.state,
            event.key,
            action,
            decision,
            engine.current_text()
        );
        // This diagnostic binary never injects, so replay every captured key.
        backend.decide(KeyboardDecision::PassThrough)?;
        if should_exit {
            break;
        }
    }
    backend.stop()
}

fn init_tracing() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).try_init()
}
