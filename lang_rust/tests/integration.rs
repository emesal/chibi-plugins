use std::process::Command;

/// Run lang_rust as a subprocess (the way chibi's indexer does) and verify the output.
#[test]
fn integration_basic_fixture() {
    let fixture = std::fs::read_to_string("tests/fixtures/basic.rs")
        .expect("fixture file missing");

    let input = serde_json::json!({
        "files": [{"path": "tests/fixtures/basic.rs", "content": fixture}]
    });

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(input.to_string().as_bytes())?;
            child.wait_with_output()
        })
        .expect("failed to run lang_rust");

    assert!(output.status.success(), "lang_rust exited with error: {}", String::from_utf8_lossy(&output.stderr));

    let result: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("invalid JSON output");

    let symbols = result["symbols"].as_array().unwrap();
    let refs = result["refs"].as_array().unwrap();

    // Verify key symbols exist.
    let sym_names: Vec<&str> = symbols.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert!(sym_names.contains(&"Parser"));
    assert!(sym_names.contains(&"Token"));
    assert!(sym_names.contains(&"Parseable"));
    assert!(sym_names.contains(&"new"));
    assert!(sym_names.contains(&"tokenize"));
    assert!(sym_names.contains(&"MAX_DEPTH"));
    assert!(sym_names.contains(&"INSTANCE_COUNT"));
    assert!(sym_names.contains(&"ParseResult"));
    assert!(sym_names.contains(&"parse_assert"));
    assert!(sym_names.contains(&"internal"));
    assert!(sym_names.contains(&"helper"));

    // Verify refs from use statements.
    let ref_names: Vec<&str> = refs.iter().map(|r| r["to_name"].as_str().unwrap()).collect();
    assert!(ref_names.contains(&"std::collections::HashMap"));
    assert!(ref_names.contains(&"crate::utils::Helper"));
    assert!(ref_names.contains(&"crate::utils::Config"));
    assert_eq!(refs.len(), 3);

    // Verify parent relationships.
    let find_sym = |name: &str, kind: &str| {
        symbols.iter().find(|s| s["name"].as_str().unwrap() == name && s["kind"].as_str().unwrap() == kind)
    };
    assert_eq!(find_sym("input", "field").unwrap()["parent"].as_str(), Some("Parser"));
    assert_eq!(find_sym("Word", "variant").unwrap()["parent"].as_str(), Some("Token"));
    assert_eq!(find_sym("helper", "function").unwrap()["parent"].as_str(), Some("internal"));
}

#[test]
fn integration_schema_mode() {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "--schema"])
        .output()
        .expect("failed to run lang_rust --schema");

    assert!(output.status.success());

    let schema: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("invalid schema JSON");

    assert_eq!(schema["name"].as_str(), Some("lang_rust"));
    assert!(schema["parameters"]["properties"]["files"].is_object());
}

#[test]
fn integration_empty_input() {
    let input = serde_json::json!({"files": [{"path": "empty.rs", "content": ""}]});

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(input.to_string().as_bytes())?;
            child.wait_with_output()
        })
        .expect("failed to run lang_rust");

    assert!(output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["symbols"].as_array().unwrap().len(), 0);
    assert_eq!(result["refs"].as_array().unwrap().len(), 0);
}
