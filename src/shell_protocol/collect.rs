use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use super::message::PREFIX;

/// Output destination for non-protocol lines.
#[derive(Clone, Copy)]
pub enum Output {
    Stdout,
    Stderr,
}

/// Read lines from stdin, write protocol messages (prefix stripped) to `proto_file`,
/// and forward all other lines to `output`.
pub fn run(proto_file: &Path, output: Output) -> Result<(), String> {
    let stdin = io::stdin();
    let mut proto =
        File::create(proto_file).map_err(|e| format!("Failed to create protocol file: {e}"))?;

    for line in BufReader::new(stdin.lock()).lines() {
        let line = line.map_err(|e| format!("Failed to read stdin: {e}"))?;
        if let Some(msg) = line.strip_prefix(PREFIX) {
            writeln!(proto, "{}", msg).map_err(|e| format!("Failed to write protocol file: {e}"))?;
        } else {
            match output {
                Output::Stdout => println!("{}", line),
                Output::Stderr => eprintln!("{}", line),
            }
        }
    }

    Ok(())
}
