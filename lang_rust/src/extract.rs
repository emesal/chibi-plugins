//! AST walk over tree-sitter parse tree to extract symbols and references.
//!
//! Strategy: depth-first traversal with parent stack. See spec for full details:
//! `chibi/chibi/docs/superpowers/specs/2026-03-16-lang-rust-plugin-design.md`

use crate::types::Output;

/// Extract symbols and references from rust source code.
pub fn extract(source: &str) -> Output {
    let _ = source;
    Output::default()
}
