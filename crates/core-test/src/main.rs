use std::{
    env,
    io::{self, Read},
};
use unicode_segmentation::UnicodeSegmentation;
use vietnamese_core::{EngineAction, EngineConfig, InputEngine, InputMethod, KeyEvent};

fn main() -> io::Result<()> {
    let mut arguments: Vec<String> = env::args().skip(1).collect();
    let input_method = if arguments.first().is_some_and(|arg| arg == "--vni") {
        arguments.remove(0);
        InputMethod::Vni
    } else {
        InputMethod::Telex
    };
    let input = if !arguments.is_empty() {
        arguments.join(" ")
    } else {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        input.trim_end_matches(['\r', '\n']).to_owned()
    };

    println!("{}", type_text(&input, input_method));
    Ok(())
}

fn type_text(input: &str, input_method: InputMethod) -> String {
    let mut engine = InputEngine::new(EngineConfig {
        input_method,
        ..EngineConfig::default()
    });
    let mut output = String::new();
    for character in input.chars() {
        match engine.process_key(KeyEvent::character(character)) {
            EngineAction::Consume => {}
            EngineAction::PassThrough => output.push(character),
            EngineAction::Commit(text) => output.push_str(&text),
            EngineAction::Replace {
                delete_graphemes,
                text,
            } => {
                for _ in 0..delete_graphemes {
                    let len = output.grapheme_indices(true).next_back().map_or(0, |x| x.0);
                    output.truncate(len);
                }
                output.push_str(&text);
            }
        }
    }
    output
}
