//! lang_rust — chibi language plugin for Rust.
//!
//! Extracts symbols and references from Rust source files using tree-sitter.
//! Conforms to chibi's language plugin protocol:
//! - `--schema`: print tool schema JSON and exit 0
//! - stdin: `{"files": [{"path": "...", "content": "..."}]}`
//! - stdout: `{"symbols": [...], "refs": [...]}`

mod extract;
mod types;

use types::{Input, Output};

fn main() {
    // Schema mode: print tool schema and exit.
    if std::env::args().any(|a| a == "--schema") {
        print_schema();
        return;
    }

    // Execution mode: read input from stdin, extract, write output to stdout.
    let input: Input = match serde_json::from_reader(std::io::stdin()) {
        Ok(input) => input,
        Err(e) => {
            eprintln!("lang_rust: failed to parse input: {}", e);
            std::process::exit(1);
        }
    };

    let mut combined = Output::default();
    for file in &input.files {
        let result = extract::extract(&file.content);
        combined.symbols.extend(result.symbols);
        combined.refs.extend(result.refs);
    }

    serde_json::to_writer(std::io::stdout(), &combined).unwrap_or_else(|e| {
        eprintln!("lang_rust: failed to write output: {}", e);
        std::process::exit(1);
    });
}

fn print_schema() {
    let schema = serde_json::json!({
        "name": "lang_rust",
        "description": "Extracts symbols and references from Rust source files using tree-sitter",
        "parameters": {
            "type": "object",
            "properties": {
                "files": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "content": { "type": "string" }
                        },
                        "required": ["path", "content"]
                    }
                }
            },
            "required": ["files"]
        }
    });
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
