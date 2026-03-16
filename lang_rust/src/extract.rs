//! AST walk over tree-sitter parse tree to extract symbols and references.
//!
//! Strategy: depth-first traversal with parent stack. See spec for full details:
//! `chibi/chibi/docs/superpowers/specs/2026-03-16-lang-rust-plugin-design.md`

use crate::types::{Output, Ref, Symbol};
use tree_sitter::{Node, Parser};

/// Extract symbols and references from rust source code.
pub fn extract(source: &str) -> Output {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("tree-sitter-rust language load failed");

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Output::default(),
    };

    let mut output = Output::default();
    let source_bytes = source.as_bytes();

    walk_node(tree.root_node(), source_bytes, &mut output, &mut Vec::new());

    output
}

/// Body child node kinds excluded from signature text.
fn is_body_node(kind: &str) -> bool {
    matches!(
        kind,
        "block"
            | "declaration_list"
            | "field_declaration_list"
            | "enum_variant_list"
            | "ordered_field_declaration_list"
    ) || kind.ends_with("_list")
}

/// Extract visibility, defaulting to "private" if no modifier present.
fn extract_visibility_default_private(node: Node, source: &[u8]) -> String {
    let mut c = node.walk();
    let children: Vec<_> = node.children(&mut c).collect();
    children.iter()
        .find(|n| n.kind() == "visibility_modifier")
        .map(|vis| {
            let text = vis.utf8_text(source).unwrap_or("").trim();
            match text {
                "pub" => "public".to_string(),
                t if t.starts_with("pub(") => t.to_string(),
                _ => "private".to_string(),
            }
        })
        .unwrap_or_else(|| "private".to_string())
}

/// Extract signature: source text from node start up to (but not including) the first body child.
fn extract_signature(node: Node, source: &[u8]) -> Option<String> {
    let node_start = node.start_byte();

    // Find the first body child byte offset.
    let mut cursor = node.walk();
    let end_byte = node
        .children(&mut cursor)
        .find(|child| is_body_node(child.kind()))
        .map(|body| body.start_byte())
        .unwrap_or(node.end_byte());

    let raw = std::str::from_utf8(&source[node_start..end_byte])
        .unwrap_or("")
        .trim();

    // Collapse whitespace/newlines to single spaces.
    let collapsed = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed)
    }
}

/// Extract name for impl_item: "Trait for Type" or "Type", stripping generic params.
fn extract_impl_name(node: Node, source: &[u8]) -> String {
    let trait_node = node.child_by_field_name("trait");
    let type_node = node.child_by_field_name("type");

    let type_name = type_node
        .map(|n| strip_generics(n.utf8_text(source).unwrap_or("").trim()))
        .unwrap_or_default();

    if let Some(trait_n) = trait_node {
        let trait_name = strip_generics(trait_n.utf8_text(source).unwrap_or("").trim());
        format!("{} for {}", trait_name, type_name)
    } else {
        type_name
    }
}

/// Strip generic parameters from a type name: "Foo<T>" → "Foo", "T" → "T".
fn strip_generics(s: &str) -> String {
    if let Some(idx) = s.find('<') {
        s[..idx].to_string()
    } else {
        s.to_string()
    }
}

/// Depth-first walk. `parent_stack` holds the current ancestor name chain.
fn walk_node(node: Node, source: &[u8], output: &mut Output, parent_stack: &mut Vec<String>) {
    let kind = node.kind();

    match kind {
        "use_declaration" => {
            let line = node.start_position().row + 1;
            extract_use_refs(node, source, line, "", output);
            // Don't recurse into use_declaration children with walk_node.
            return;
        }

        "function_item" | "function_signature_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let visibility = extract_visibility_default_private(node, source);
                let signature = extract_signature(node, source);
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name,
                    kind: "function".to_string(),
                    line_start,
                    line_end,
                    signature,
                    visibility: Some(visibility),
                    parent,
                });
            }
            // Functions are not parents — don't push to stack, but do recurse for nested fns.
        }

        "struct_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let visibility = extract_visibility_default_private(node, source);
                let signature = extract_signature(node, source);
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name: name.clone(),
                    kind: "struct".to_string(),
                    line_start,
                    line_end,
                    signature,
                    visibility: Some(visibility),
                    parent,
                });

                parent_stack.push(name);
                recurse_children(node, source, output, parent_stack);
                parent_stack.pop();
                return;
            }
        }

        "enum_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let visibility = extract_visibility_default_private(node, source);
                let signature = extract_signature(node, source);
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name: name.clone(),
                    kind: "enum".to_string(),
                    line_start,
                    line_end,
                    signature,
                    visibility: Some(visibility),
                    parent,
                });

                parent_stack.push(name);
                recurse_children(node, source, output, parent_stack);
                parent_stack.pop();
                return;
            }
        }

        "enum_variant" => {
            // Name is the identifier child.
            let name = if let Some(n) = node.child_by_field_name("name") {
                n.utf8_text(source).unwrap_or("").to_string()
            } else {
                // fallback: first identifier child
                let mut c = node.walk();
                let children: Vec<_> = node.children(&mut c).collect();
                children.iter()
                    .find(|n| n.kind() == "identifier")
                    .map(|n| n.utf8_text(source).unwrap_or("").to_string())
                    .unwrap_or_default()
            };

            if !name.is_empty() {
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name,
                    kind: "variant".to_string(),
                    line_start,
                    line_end,
                    signature: None,
                    visibility: None,
                    parent,
                });
            }
            return;
        }

        "union_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let visibility = extract_visibility_default_private(node, source);
                let signature = extract_signature(node, source);
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name: name.clone(),
                    kind: "union".to_string(),
                    line_start,
                    line_end,
                    signature,
                    visibility: Some(visibility),
                    parent,
                });

                parent_stack.push(name);
                recurse_children(node, source, output, parent_stack);
                parent_stack.pop();
                return;
            }
        }

        "trait_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let visibility = extract_visibility_default_private(node, source);
                let signature = extract_signature(node, source);
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name: name.clone(),
                    kind: "trait".to_string(),
                    line_start,
                    line_end,
                    signature,
                    visibility: Some(visibility),
                    parent,
                });

                parent_stack.push(name);
                recurse_children(node, source, output, parent_stack);
                parent_stack.pop();
                return;
            }
        }

        "impl_item" => {
            let name = extract_impl_name(node, source);
            let line_start = node.start_position().row + 1;
            let line_end = node.end_position().row + 1;
            let signature = extract_signature(node, source);
            let parent = parent_stack.last().cloned();

            output.symbols.push(Symbol {
                name: name.clone(),
                kind: "impl".to_string(),
                line_start,
                line_end,
                signature,
                visibility: None,
                parent,
            });

            parent_stack.push(name);
            recurse_children(node, source, output, parent_stack);
            parent_stack.pop();
            return;
        }

        "mod_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let visibility = extract_visibility_default_private(node, source);
                let signature = extract_signature(node, source);
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name: name.clone(),
                    kind: "module".to_string(),
                    line_start,
                    line_end,
                    signature,
                    visibility: Some(visibility),
                    parent,
                });

                parent_stack.push(name);
                recurse_children(node, source, output, parent_stack);
                parent_stack.pop();
                return;
            }
        }

        "type_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let visibility = extract_visibility_default_private(node, source);
                let signature = extract_signature(node, source);
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name,
                    kind: "type".to_string(),
                    line_start,
                    line_end,
                    signature,
                    visibility: Some(visibility),
                    parent,
                });
            }
            return;
        }

        "const_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let visibility = extract_visibility_default_private(node, source);
                let signature = extract_signature(node, source);
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name,
                    kind: "constant".to_string(),
                    line_start,
                    line_end,
                    signature,
                    visibility: Some(visibility),
                    parent,
                });
            }
            return;
        }

        "static_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source).unwrap_or("").to_string();
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let visibility = extract_visibility_default_private(node, source);
                let signature = extract_signature(node, source);
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name,
                    kind: "static".to_string(),
                    line_start,
                    line_end,
                    signature,
                    visibility: Some(visibility),
                    parent,
                });
            }
            return;
        }

        "macro_definition" => {
            // macro_rules! name { ... } — name is identifier child
            let name = {
                let mut c = node.walk();
                let children: Vec<_> = node.children(&mut c).collect();
                children.iter()
                    .find(|n| n.kind() == "identifier")
                    .map(|n| n.utf8_text(source).unwrap_or("").to_string())
                    .unwrap_or_default()
            };
            if !name.is_empty() {
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name,
                    kind: "macro".to_string(),
                    line_start,
                    line_end,
                    signature: None,
                    visibility: None,
                    parent,
                });
            }
            return;
        }

        "field_declaration" => {
            // Field name is a field_identifier child.
            let name = {
                let mut c = node.walk();
                let children: Vec<_> = node.children(&mut c).collect();
                children.iter()
                    .find(|n| n.kind() == "field_identifier" || n.kind() == "identifier")
                    .map(|n| n.utf8_text(source).unwrap_or("").to_string())
                    .unwrap_or_default()
            };
            if !name.is_empty() {
                let line_start = node.start_position().row + 1;
                let line_end = node.end_position().row + 1;
                let visibility = extract_visibility_default_private(node, source);
                let signature = node
                    .utf8_text(source)
                    .map(|s| s.trim().to_string())
                    .ok()
                    .filter(|s| !s.is_empty());
                let parent = parent_stack.last().cloned();

                output.symbols.push(Symbol {
                    name,
                    kind: "field".to_string(),
                    line_start,
                    line_end,
                    signature,
                    visibility: Some(visibility),
                    parent,
                });
            }
            return;
        }

        _ => {}
    }

    // Default: recurse into children.
    recurse_children(node, source, output, parent_stack);
}

/// Recurse into all children of a node.
fn recurse_children(node: Node, source: &[u8], output: &mut Output, parent_stack: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_node(child, source, output, parent_stack);
    }
}

/// Recursively extract refs from a use_declaration or use subtree.
/// `prefix` is the path accumulated so far (e.g. "std::collections").
fn extract_use_refs(node: Node, source: &[u8], line: usize, prefix: &str, output: &mut Output) {
    match node.kind() {
        "use_declaration" => {
            // Child is the use tree (scoped_identifier, use_list, scoped_use_list, etc.)
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "use" && child.kind() != ";" {
                    extract_use_refs(child, source, line, prefix, output);
                }
            }
        }

        "scoped_use_list" => {
            // e.g. std::collections::{HashMap, BTreeMap}
            // Has "path" field and "list" field.
            let path_node = node.child_by_field_name("path");
            let list_node = node.child_by_field_name("list");

            let new_prefix = if let Some(p) = path_node {
                let path_text = p.utf8_text(source).unwrap_or("").trim();
                if prefix.is_empty() {
                    path_text.to_string()
                } else {
                    format!("{}::{}", prefix, path_text)
                }
            } else {
                prefix.to_string()
            };

            if let Some(list) = list_node {
                extract_use_refs(list, source, line, &new_prefix, output);
            }
        }

        "use_list" => {
            // Braced list: {A, B, C}
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "{" && child.kind() != "}" && child.kind() != "," {
                    extract_use_refs(child, source, line, prefix, output);
                }
            }
        }

        "scoped_identifier" => {
            // Full path like std::collections::HashMap
            let text = node.utf8_text(source).unwrap_or("").trim();
            let full_path = if prefix.is_empty() {
                text.to_string()
            } else {
                format!("{}::{}", prefix, text)
            };
            output.refs.push(Ref {
                from_line: line,
                to_name: full_path,
                kind: "import".to_string(),
            });
        }

        "identifier" => {
            let text = node.utf8_text(source).unwrap_or("").trim();
            if text.is_empty() {
                return;
            }
            let full_path = if prefix.is_empty() {
                text.to_string()
            } else {
                format!("{}::{}", prefix, text)
            };
            output.refs.push(Ref {
                from_line: line,
                to_name: full_path,
                kind: "import".to_string(),
            });
        }

        "use_wildcard" => {
            // use_wildcard contains the full path + "*", e.g. "std::collections::*"
            // Just take the full text of the node.
            let text = node.utf8_text(source).unwrap_or("").trim();
            let full_path = if prefix.is_empty() {
                text.to_string()
            } else {
                format!("{}::{}", prefix, text)
            };
            output.refs.push(Ref {
                from_line: line,
                to_name: full_path,
                kind: "import".to_string(),
            });
        }

        "use_as_clause" => {
            // path as alias — emit the original path, not the alias.
            let path_node = node.child_by_field_name("path");
            if let Some(p) = path_node {
                extract_use_refs(p, source, line, prefix, output);
            } else {
                // Fallback: first non-keyword child.
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                if let Some(first) = children.into_iter().next() {
                    extract_use_refs(first, source, line, prefix, output);
                }
            }
        }

        _ => {
            // Unknown node inside a use tree — try recursing.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                extract_use_refs(child, source, line, prefix, output);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_function() {
        let src = "pub fn hello(name: &str) -> String {\n    format!(\"hi {}\", name)\n}";
        let out = extract(src);
        assert_eq!(out.symbols.len(), 1);
        let sym = &out.symbols[0];
        assert_eq!(sym.name, "hello");
        assert_eq!(sym.kind, "function");
        assert_eq!(sym.line_start, 1);
        assert_eq!(sym.line_end, 3);
        assert_eq!(sym.visibility.as_deref(), Some("public"));
        assert_eq!(
            sym.signature.as_deref(),
            Some("pub fn hello(name: &str) -> String")
        );
        assert_eq!(sym.parent, None);
    }

    #[test]
    fn extract_struct() {
        let src = "pub struct Config {\n    name: String,\n    value: i32,\n}";
        let out = extract(src);
        // struct + 2 fields
        assert_eq!(out.symbols.len(), 3);
        assert_eq!(out.symbols[0].name, "Config");
        assert_eq!(out.symbols[0].kind, "struct");
        assert_eq!(out.symbols[0].visibility.as_deref(), Some("public"));
        assert_eq!(out.symbols[0].signature.as_deref(), Some("pub struct Config"));
    }

    #[test]
    fn extract_enum_with_variants() {
        let src = "pub enum Color {\n    Red,\n    Green,\n    Blue,\n}";
        let out = extract(src);
        assert_eq!(out.symbols.len(), 4); // enum + 3 variants
        assert_eq!(out.symbols[0].name, "Color");
        assert_eq!(out.symbols[0].kind, "enum");
        assert_eq!(out.symbols[1].kind, "variant");
        assert_eq!(out.symbols[1].parent.as_deref(), Some("Color"));
    }

    #[test]
    fn extract_trait() {
        let src = "pub trait Display {\n    fn fmt(&self) -> String;\n}";
        let out = extract(src);
        assert_eq!(out.symbols.len(), 2); // trait + method signature
        assert_eq!(out.symbols[0].name, "Display");
        assert_eq!(out.symbols[0].kind, "trait");
        assert_eq!(out.symbols[1].name, "fmt");
        assert_eq!(out.symbols[1].kind, "function");
        assert_eq!(out.symbols[1].parent.as_deref(), Some("Display"));
    }

    #[test]
    fn extract_const_and_static() {
        let src = "pub const MAX: usize = 100;\nstatic COUNTER: i32 = 0;";
        let out = extract(src);
        assert_eq!(out.symbols.len(), 2);
        assert_eq!(out.symbols[0].name, "MAX");
        assert_eq!(out.symbols[0].kind, "constant");
        assert_eq!(out.symbols[1].name, "COUNTER");
        assert_eq!(out.symbols[1].kind, "static");
    }

    #[test]
    fn extract_type_alias() {
        let src = "pub type Result<T> = std::result::Result<T, Error>;";
        let out = extract(src);
        assert_eq!(out.symbols.len(), 1);
        assert_eq!(out.symbols[0].name, "Result");
        assert_eq!(out.symbols[0].kind, "type");
    }

    #[test]
    fn extract_mod() {
        let src = "pub mod parser {\n    pub fn parse() {}\n}";
        let out = extract(src);
        assert_eq!(out.symbols.len(), 2); // mod + fn
        assert_eq!(out.symbols[0].name, "parser");
        assert_eq!(out.symbols[0].kind, "module");
        assert_eq!(out.symbols[1].name, "parse");
        assert_eq!(out.symbols[1].parent.as_deref(), Some("parser"));
    }

    #[test]
    fn extract_macro_definition() {
        let src = "macro_rules! my_macro {\n    () => {};\n}";
        let out = extract(src);
        assert_eq!(out.symbols.len(), 1);
        assert_eq!(out.symbols[0].name, "my_macro");
        assert_eq!(out.symbols[0].kind, "macro");
    }

    #[test]
    fn extract_union() {
        let src = "pub union MyUnion {\n    f: f32,\n    i: i32,\n}";
        let out = extract(src);
        assert_eq!(out.symbols.len(), 3); // union + 2 fields
        assert_eq!(out.symbols[0].name, "MyUnion");
        assert_eq!(out.symbols[0].kind, "union");
        assert_eq!(out.symbols[1].kind, "field");
        assert_eq!(out.symbols[1].parent.as_deref(), Some("MyUnion"));
    }

    #[test]
    fn extract_private_function() {
        let src = "fn helper() {}";
        let out = extract(src);
        assert_eq!(out.symbols[0].visibility.as_deref(), Some("private"));
    }

    #[test]
    fn extract_pub_crate_visibility() {
        let src = "pub(crate) fn internal() {}";
        let out = extract(src);
        assert_eq!(out.symbols[0].visibility.as_deref(), Some("pub(crate)"));
    }

    #[test]
    fn extract_pub_super_visibility() {
        let src = "pub(super) fn parent_visible() {}";
        let out = extract(src);
        assert_eq!(out.symbols[0].visibility.as_deref(), Some("pub(super)"));
    }

    #[test]
    fn extract_tuple_struct_no_fields() {
        // Tuple struct fields (ordered_field_declaration_list) are not extracted in v1.
        // Only the struct itself should appear.
        let src = "pub struct Pair(i32, i32);";
        let out = extract(src);
        assert_eq!(out.symbols.len(), 1);
        assert_eq!(out.symbols[0].name, "Pair");
        assert_eq!(out.symbols[0].kind, "struct");
    }

    #[test]
    fn extract_empty_file() {
        let out = extract("");
        assert!(out.symbols.is_empty());
        assert!(out.refs.is_empty());
    }

    // Impl block edge cases

    #[test]
    fn extract_impl_block_methods() {
        let src = "struct Foo {}\n\nimpl Foo {\n    pub fn new() -> Self { Foo {} }\n    fn helper(&self) {}\n}";
        let out = extract(src);
        // struct + impl + 2 methods
        let impl_sym = out.symbols.iter().find(|s| s.kind == "impl").unwrap();
        assert_eq!(impl_sym.name, "Foo");
        assert_eq!(impl_sym.signature.as_deref(), Some("impl Foo"));

        // Both methods have parent "Foo".
        let methods: Vec<_> = out.symbols.iter().filter(|s| s.kind == "function" && s.parent.as_deref() == Some("Foo")).collect();
        assert_eq!(methods.len(), 2);
        assert_eq!(methods[0].name, "new");
        assert_eq!(methods[0].visibility.as_deref(), Some("public"));
        assert_eq!(methods[1].name, "helper");
        assert_eq!(methods[1].visibility.as_deref(), Some("private"));
    }

    #[test]
    fn extract_impl_trait_for_type() {
        let src = "impl Display for Foo {\n    fn fmt(&self) -> String { String::new() }\n}";
        let out = extract(src);
        let impl_sym = out.symbols.iter().find(|s| s.kind == "impl").unwrap();
        assert_eq!(impl_sym.name, "Display for Foo");

        let method = out.symbols.iter().find(|s| s.kind == "function").unwrap();
        assert_eq!(method.parent.as_deref(), Some("Display for Foo"));
    }

    #[test]
    fn extract_generic_impl_strips_params() {
        let src = "impl<T> Foo<T> {\n    fn bar(&self) {}\n}";
        let out = extract(src);
        let impl_sym = out.symbols.iter().find(|s| s.kind == "impl").unwrap();
        assert_eq!(impl_sym.name, "Foo");
    }

    #[test]
    fn extract_generic_trait_for_type_strips_params() {
        let src = "impl<T: Display> ToString for T {\n    fn to_string(&self) -> String { String::new() }\n}";
        let out = extract(src);
        let impl_sym = out.symbols.iter().find(|s| s.kind == "impl").unwrap();
        assert_eq!(impl_sym.name, "ToString for T");
    }

    #[test]
    fn extract_multiple_impl_blocks_same_type() {
        let src = "struct S {}\nimpl S {\n    fn a(&self) {}\n}\nimpl S {\n    fn b(&self) {}\n}";
        let out = extract(src);
        let methods: Vec<_> = out.symbols.iter().filter(|s| s.kind == "function").collect();
        assert_eq!(methods.len(), 2);
        assert!(methods.iter().all(|m| m.parent.as_deref() == Some("S")));
    }

    // Edge cases

    #[test]
    fn extract_syntax_error_partial_tree() {
        // tree-sitter produces a partial tree for incomplete code.
        let src = "pub fn valid() {}\n\npub fn broken( {}\n\npub struct Good {}";
        let out = extract(src);
        let names: Vec<&str> = out.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"valid"));
        assert!(names.contains(&"Good"));
    }

    #[test]
    fn extract_deeply_nested_modules() {
        let src = "mod a {\n    mod b {\n        pub fn deep() {}\n    }\n}";
        let out = extract(src);
        let deep = out.symbols.iter().find(|s| s.name == "deep").unwrap();
        assert_eq!(deep.parent.as_deref(), Some("b"));
        let b = out.symbols.iter().find(|s| s.name == "b").unwrap();
        assert_eq!(b.parent.as_deref(), Some("a"));
    }

    #[test]
    fn extract_struct_fields_as_children() {
        let src = "pub struct Point {\n    pub x: f64,\n    pub y: f64,\n}";
        let out = extract(src);
        let fields: Vec<_> = out.symbols.iter().filter(|s| s.kind == "field").collect();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "x");
        assert_eq!(fields[0].parent.as_deref(), Some("Point"));
        assert_eq!(fields[0].visibility.as_deref(), Some("public"));
        assert_eq!(fields[1].name, "y");
    }

    #[test]
    fn extract_trait_with_default_and_signature_methods() {
        let src = "trait MyTrait {\n    fn required(&self);\n    fn optional(&self) { }\n}";
        let out = extract(src);
        let methods: Vec<_> = out.symbols.iter().filter(|s| s.kind == "function").collect();
        assert_eq!(methods.len(), 2);
        assert!(methods.iter().all(|m| m.parent.as_deref() == Some("MyTrait")));
    }

    #[test]
    fn extract_multiline_signature() {
        let src = "pub fn complex(\n    a: i32,\n    b: String,\n) -> Result<(), Error> {\n}";
        let out = extract(src);
        let sig = out.symbols[0].signature.as_deref().unwrap();
        // Signature should be collapsed to single line, trimmed.
        assert!(sig.contains("pub fn complex("));
        assert!(sig.contains("-> Result<(), Error>"));
        assert!(!sig.contains('{'));
    }

    // Reference extraction tests

    #[test]
    fn extract_simple_use() {
        let src = "use std::collections::HashMap;";
        let out = extract(src);
        assert_eq!(out.refs.len(), 1);
        assert_eq!(out.refs[0].to_name, "std::collections::HashMap");
        assert_eq!(out.refs[0].kind, "import");
        assert_eq!(out.refs[0].from_line, 1);
    }

    #[test]
    fn extract_grouped_use() {
        let src = "use crate::parser::{Parser, Token};";
        let out = extract(src);
        assert_eq!(out.refs.len(), 2);
        let names: Vec<&str> = out.refs.iter().map(|r| r.to_name.as_str()).collect();
        assert!(names.contains(&"crate::parser::Parser"));
        assert!(names.contains(&"crate::parser::Token"));
    }

    #[test]
    fn extract_nested_grouped_use() {
        let src = "use std::{collections::{HashMap, BTreeMap}, io::Read};";
        let out = extract(src);
        assert_eq!(out.refs.len(), 3);
        let names: Vec<&str> = out.refs.iter().map(|r| r.to_name.as_str()).collect();
        assert!(names.contains(&"std::collections::HashMap"));
        assert!(names.contains(&"std::collections::BTreeMap"));
        assert!(names.contains(&"std::io::Read"));
    }

    #[test]
    fn extract_glob_use() {
        let src = "use std::collections::*;";
        let out = extract(src);
        assert_eq!(out.refs.len(), 1);
        assert_eq!(out.refs[0].to_name, "std::collections::*");
    }

    #[test]
    fn extract_aliased_use() {
        let src = "use std::io::Result as IoResult;";
        let out = extract(src);
        assert_eq!(out.refs.len(), 1);
        assert_eq!(out.refs[0].to_name, "std::io::Result");
    }

    #[test]
    fn extract_bare_use_list() {
        // Rare but legal: `use {std, core};` with no path prefix.
        let src = "use {std, core};";
        let out = extract(src);
        assert_eq!(out.refs.len(), 2);
        let names: Vec<&str> = out.refs.iter().map(|r| r.to_name.as_str()).collect();
        assert!(names.contains(&"std"));
        assert!(names.contains(&"core"));
    }

    #[test]
    fn extract_multiple_use_statements() {
        let src = "use std::io;\nuse std::fmt;\n\npub fn f() {}";
        let out = extract(src);
        assert_eq!(out.refs.len(), 2);
        assert_eq!(out.refs[0].from_line, 1);
        assert_eq!(out.refs[1].from_line, 2);
    }
}
