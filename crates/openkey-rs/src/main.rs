use std::{env, process::ExitCode};

use keyboard_linux::{
    KeyboardBackend, KeyboardDecision, X11KeyboardBackend, decision_for, execute_engine_action,
};
use tracing::error;
use tracing_subscriber::EnvFilter;
use vietnamese_core::{EngineConfig, InputEngine, InputMethod};

#[derive(Debug, Default)]
struct Options {
    debug_input: bool,
    input_method: Option<InputMethod>,
    disabled: bool,
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(Some(options)) => options,
        Ok(None) => return ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("openkey-rs: {message}\nUse --help for usage.");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = init_tracing() {
        eprintln!("openkey-rs: failed to initialize logging: {error}");
        return ExitCode::FAILURE;
    }
    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("openkey-rs: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: Options) -> keyboard_linux::Result<()> {
    let mut config = EngineConfig::default();
    if let Some(input_method) = options.input_method {
        config.input_method = input_method;
    }
    config.enabled = !options.disabled;

    let mut engine = InputEngine::new(config);
    let mut backend = X11KeyboardBackend::new()?;
    backend.start()?;

    loop {
        let event = backend.next_event()?;
        let action = engine.process_key(event);
        let decision = decision_for(&action);

        if options.debug_input {
            println!("KEY: {:?}\nACTION: {action:?}\n", event.key);
        }

        // Every synchronously grabbed event must be resolved before injection.
        backend.decide(decision)?;
        if decision == KeyboardDecision::Consume {
            let result = {
                let mut injector = backend.text_injector();
                execute_engine_action(&mut injector, &action)
            };
            if let Err(injection_error) = result {
                engine.reset();
                error!(error = %injection_error, "X11 text injection failed; composition reset");
            }
        }
    }
}

fn parse_options() -> Result<Option<Options>, String> {
    let mut options = Options::default();
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--debug-input" => options.debug_input = true,
            "--vni" => options.input_method = Some(InputMethod::Vni),
            "--telex" => options.input_method = Some(InputMethod::Telex),
            "--disabled" => options.disabled = true,
            "-h" | "--help" => {
                println!(
                    "openkey-rs Phase 3\n\n\
                     Usage: openkey-rs [--debug-input] [--telex|--vni] [--disabled]\n\n\
                     Runs the foreground X11 input loop. No daemon or IPC is started."
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

    #[test]
    fn defaults_are_telex_and_enabled() {
        let options = Options::default();
        assert!(!options.debug_input);
        assert!(!options.disabled);
        assert_eq!(options.input_method, None);
    }
}
