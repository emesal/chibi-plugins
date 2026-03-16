//! Serde types for the language plugin JSON protocol.
//!
//! Input: `{"files": [{"path": "...", "content": "..."}]}`
//! Output: `{"symbols": [...], "refs": [...]}`

use serde::{Deserialize, Serialize};

/// Top-level input from the indexer.
#[derive(Debug, Deserialize)]
pub struct Input {
    pub files: Vec<FileEntry>,
}

/// A single file to extract symbols from.
#[derive(Debug, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub content: String,
}

/// Top-level output to the indexer.
#[derive(Debug, Serialize, Default)]
pub struct Output {
    pub symbols: Vec<Symbol>,
    pub refs: Vec<Ref>,
}

/// An extracted symbol.
#[derive(Debug, Serialize, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub line_start: usize,
    pub line_end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// An extracted reference (use statement).
#[derive(Debug, Serialize, PartialEq)]
pub struct Ref {
    pub from_line: usize,
    pub to_name: String,
    pub kind: String,
}
