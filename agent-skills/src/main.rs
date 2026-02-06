//! Agent Skills plugin for chibi
//!
//! Provides a compatibility layer for the Agent Skills (https://agentskills.io) standard.
//! - Parses SKILL.md files and exposes them as chibi tools
//! - Handles skill invocation and progressive disclosure
//! - Provides marketplace functionality for installing skills
//! - Enforces allowed-tools restrictions via pre_tool hook

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode};

// ============================================================================
// Data Structures
// ============================================================================

/// Parsed skill from SKILL.md
#[derive(Debug, Clone)]
struct Skill {
    name: String,
    description: String,
    body: String,
    allowed_tools: Option<String>,
}

/// Active skill state for allowed-tools enforcement
#[derive(Serialize, Deserialize, Default)]
struct ActiveSkill {
    name: String,
    allowed_tools: Option<String>,
}

/// Hook data for pre_tool
#[derive(Deserialize, Default)]
struct PreToolHookData {
    tool_name: Option<String>,
    #[allow(dead_code)]
    arguments: Option<serde_json::Value>,
}

/// Hook response for blocking a tool
#[derive(Serialize)]
struct BlockResponse {
    block: bool,
    message: String,
}

/// Hook response for injecting content
#[derive(Serialize)]
struct InjectResponse {
    inject: String,
}

/// Tool arguments for skill_marketplace
#[derive(Deserialize, Default)]
struct MarketplaceArgs {
    action: Option<String>,
    skill_ref: Option<String>,
    query: Option<String>,
}

/// Tool arguments for read_skill_file
#[derive(Deserialize, Default)]
struct ReadSkillFileArgs {
    skill: Option<String>,
    path: Option<String>,
}

/// Tool arguments for run_skill_script
#[derive(Deserialize, Default)]
struct RunSkillScriptArgs {
    skill: Option<String>,
    script: Option<String>,
    args: Option<Vec<String>>,
    stdin: Option<String>,
}

/// Tool arguments for skill invocation
#[derive(Deserialize, Default)]
struct SkillInvocationArgs {
    arguments: Option<String>,
}

/// Installed skill info for listing
#[derive(Serialize)]
struct SkillInfo {
    name: String,
    description: String,
}

// ============================================================================
// Stdin Helper
// ============================================================================

/// Read all of stdin into a string (args/hook data are passed via stdin)
fn read_stdin() -> String {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf).unwrap_or_default();
    if buf.is_empty() { "{}".to_string() } else { buf }
}

// ============================================================================
// Path Helpers
// ============================================================================

fn plugin_dir() -> PathBuf {
    // Get the directory where this executable is located
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn skills_dir() -> PathBuf {
    plugin_dir().join("skills")
}

fn state_file() -> PathBuf {
    plugin_dir().join(".active_skill.json")
}

// ============================================================================
// SKILL.md Parsing
// ============================================================================

/// Parse YAML frontmatter from SKILL.md content
fn parse_frontmatter(content: &str) -> Option<HashMap<String, serde_yaml::Value>> {
    // Check for frontmatter markers
    if !content.starts_with("---") {
        return None;
    }

    // Find the closing marker
    let rest = &content[3..];
    let end_pos = rest.find("\n---")?;
    let yaml_str = &rest[..end_pos].trim();

    serde_yaml::from_str(yaml_str).ok()
}

/// Parse a SKILL.md file
fn parse_skill(skill_path: &PathBuf) -> Option<Skill> {
    let content = fs::read_to_string(skill_path).ok()?;

    let frontmatter = parse_frontmatter(&content)?;

    // Get required fields
    let name = frontmatter.get("name")?.as_str()?.to_string();
    let description = frontmatter.get("description")?.as_str()?.to_string();

    // Validate name format (alphanumeric + hyphens)
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return None;
    }
    if name.len() > 64 || description.len() > 1024 {
        return None;
    }

    // Extract body (everything after frontmatter)
    let body_start = content.find("\n---\n").map(|p| p + 5).unwrap_or(0);
    let body = content[body_start..].trim().to_string();

    // Get optional allowed-tools
    let allowed_tools = frontmatter
        .get("allowed-tools")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(Skill {
        name,
        description,
        body,
        allowed_tools,
    })
}

/// List all valid skills in the skills directory
fn list_skills() -> Vec<Skill> {
    let mut skills = Vec::new();
    let dir = skills_dir();

    if !dir.exists() {
        return skills;
    }

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !path.file_name().map_or(true, |n| n.to_string_lossy().starts_with('.')) {
                let skill_path = path.join("SKILL.md");
                if let Some(skill) = parse_skill(&skill_path) {
                    skills.push(skill);
                }
            }
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

// ============================================================================
// State Management
// ============================================================================

fn get_active_skill() -> Option<ActiveSkill> {
    let path = state_file();
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn set_active_skill(name: &str, allowed_tools: Option<String>) {
    let state = ActiveSkill {
        name: name.to_string(),
        allowed_tools,
    };
    if let Ok(json) = serde_json::to_string(&state) {
        let _ = fs::write(state_file(), json);
    }
}

fn clear_active_skill() {
    let path = state_file();
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

// ============================================================================
// Allowed Tools Checking
// ============================================================================

fn is_tool_allowed(tool_name: &str, allowed_tools: &str) -> bool {
    // Parse allowed-tools string: "Read, Grep, Bash(git:*)"
    let allowed_list: Vec<&str> = allowed_tools.split(',').map(|s| s.trim()).collect();

    for allowed in allowed_list {
        if allowed.contains('(') {
            // Pattern match: Bash(git:*) - for now, allow if base matches
            let base = allowed.split('(').next().unwrap_or("");
            if tool_name == base {
                return true;
            }
        } else if tool_name == allowed {
            return true;
        }
    }

    // Always allow agent-skills tools themselves
    if matches!(
        tool_name,
        "skill_marketplace" | "read_skill_file" | "run_skill_script"
    ) {
        return true;
    }
    if tool_name.starts_with("skill_") {
        return true;
    }

    false
}

// ============================================================================
// Schema Generation
// ============================================================================

fn generate_schema() -> serde_json::Value {
    let mut tools = Vec::new();

    // Core management tools
    tools.push(serde_json::json!({
        "name": "skill_marketplace",
        "description": "Install, remove, search, or list Agent Skills from the marketplace",
        "parameters": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["install", "remove", "search", "list", "list_installed"],
                    "description": "Action to perform"
                },
                "skill_ref": {
                    "type": "string",
                    "description": "Skill reference (owner/name) for install/remove"
                },
                "query": {
                    "type": "string",
                    "description": "Search query for search action"
                }
            },
            "required": ["action"]
        },
        "hooks": ["post_system_prompt", "pre_tool", "on_start"]
    }));

    tools.push(serde_json::json!({
        "name": "read_skill_file",
        "description": "Read a file from an installed skill's directory (scripts, references, etc.)",
        "parameters": {
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "Name of the installed skill"
                },
                "path": {
                    "type": "string",
                    "description": "Relative path to the file within the skill directory"
                }
            },
            "required": ["skill", "path"]
        }
    }));

    tools.push(serde_json::json!({
        "name": "run_skill_script",
        "description": "Execute a script from an installed skill's directory (e.g., scripts/extract.py)",
        "parameters": {
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "Name of the installed skill"
                },
                "script": {
                    "type": "string",
                    "description": "Relative path to the script within the skill directory"
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Arguments to pass to the script (optional)"
                },
                "stdin": {
                    "type": "string",
                    "description": "Input to pass to the script via stdin (optional)"
                }
            },
            "required": ["skill", "script"]
        }
    }));

    // One tool per installed skill
    for skill in list_skills() {
        tools.push(serde_json::json!({
            "name": format!("skill_{}", skill.name),
            "description": skill.description,
            "parameters": {
                "type": "object",
                "properties": {
                    "arguments": {
                        "type": "string",
                        "description": "Arguments to pass to the skill (optional)"
                    }
                }
            }
        }));
    }

    serde_json::Value::Array(tools)
}

// ============================================================================
// Hook Handlers
// ============================================================================

fn handle_on_start_hook() {
    clear_active_skill();
    println!("{{}}");
}

fn handle_post_system_prompt_hook() {
    let skills = list_skills();
    if skills.is_empty() {
        println!("{{}}");
        return;
    }

    let mut lines = vec!["## Available Agent Skills".to_string(), String::new()];
    for skill in skills {
        lines.push(format!("- **{}**: {}", skill.name, skill.description));
    }
    lines.push(String::new());
    lines.push("Use skill_[name] tools to invoke a skill and receive detailed instructions.".to_string());

    let response = InjectResponse {
        inject: lines.join("\n"),
    };
    println!("{}", serde_json::to_string(&response).unwrap());
}

fn handle_pre_tool_hook(stdin_data: &str) {
    let hook_data: PreToolHookData = serde_json::from_str(stdin_data).unwrap_or_default();

    let tool_name = hook_data.tool_name.unwrap_or_default();

    // Track skill activation
    if tool_name.starts_with("skill_") && tool_name != "skill_marketplace" {
        let skill_name = &tool_name[6..]; // Remove "skill_" prefix
        let skill_path = skills_dir().join(skill_name).join("SKILL.md");
        if let Some(skill) = parse_skill(&skill_path) {
            set_active_skill(skill_name, skill.allowed_tools);
            println!("{{}}");
            return;
        }
    }

    // Enforce allowed-tools for active skill
    if let Some(active) = get_active_skill() {
        if let Some(allowed) = &active.allowed_tools {
            if !is_tool_allowed(&tool_name, allowed) {
                let response = BlockResponse {
                    block: true,
                    message: format!(
                        "Tool '{}' is not allowed while skill '{}' is active. Allowed tools: {}",
                        tool_name, active.name, allowed
                    ),
                };
                println!("{}", serde_json::to_string(&response).unwrap());
                return;
            }
        }
    }

    println!("{{}}");
}

// ============================================================================
// Tool Handlers
// ============================================================================

fn handle_marketplace(args: MarketplaceArgs) {
    let action = args.action.unwrap_or_default();

    match action.as_str() {
        "install" => {
            let skill_ref = match args.skill_ref {
                Some(r) => r,
                None => {
                    println!("Error: skill_ref required for install");
                    return;
                }
            };
            handle_install(&skill_ref);
        }
        "remove" => {
            let skill_ref = match args.skill_ref {
                Some(r) => r,
                None => {
                    println!("Error: skill_ref required for remove");
                    return;
                }
            };
            handle_remove(&skill_ref);
        }
        "search" => {
            let query = args.query.unwrap_or_default();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!([{
                    "message": format!("Marketplace search not yet implemented. Check https://github.com/anthropics/skills for available skills matching '{}'.", query)
                }]))
                .unwrap()
            );
        }
        "list" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!([{
                    "message": "Marketplace listing not yet implemented. Check https://github.com/anthropics/skills for available skills."
                }]))
                .unwrap()
            );
        }
        "list_installed" => {
            let skills = list_skills();
            if skills.is_empty() {
                println!("No skills installed. Use 'install' action to add skills.");
            } else {
                let infos: Vec<SkillInfo> = skills
                    .into_iter()
                    .map(|s| SkillInfo {
                        name: s.name,
                        description: s.description,
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&infos).unwrap());
            }
        }
        _ => {
            println!("Error: Unknown action '{}'", action);
        }
    }
}

fn handle_install(skill_ref: &str) {
    let skills_dir = skills_dir();
    let _ = fs::create_dir_all(&skills_dir);

    // Parse skill reference
    let (repo_url, skill_name) = if skill_ref.starts_with("http") {
        let name = skill_ref.trim_end_matches('/').rsplit('/').next().unwrap_or(skill_ref);
        (skill_ref.to_string(), name.to_string())
    } else if skill_ref.contains('/') {
        let parts: Vec<&str> = skill_ref.splitn(2, '/').collect();
        let owner = parts[0];
        let name = parts[1];
        (
            format!("https://github.com/{}/skills", owner),
            name.to_string(),
        )
    } else {
        println!("Error: Invalid skill reference '{}'. Use 'owner/skill-name' format.", skill_ref);
        return;
    };

    let target_dir = skills_dir.join(&skill_name);
    if target_dir.exists() {
        println!(
            "Skill '{}' is already installed. Remove it first to reinstall.",
            skill_name
        );
        return;
    }

    // Clone with sparse checkout
    let temp_dir = skills_dir.join(format!(".tmp_{}", skill_name));
    if temp_dir.exists() {
        let _ = fs::remove_dir_all(&temp_dir);
    }

    let clone_result = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--filter=blob:none",
            "--sparse",
            &repo_url,
            temp_dir.to_str().unwrap(),
        ])
        .output();

    match clone_result {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let _ = fs::remove_dir_all(&temp_dir);
            println!(
                "Error cloning repository: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&temp_dir);
            println!("Error running git: {}", e);
            return;
        }
    }

    // Set up sparse checkout
    let _ = Command::new("git")
        .args([
            "-C",
            temp_dir.to_str().unwrap(),
            "sparse-checkout",
            "set",
            &format!("skills/{}", skill_name),
        ])
        .output();

    // Move skill to target location
    let skill_source = temp_dir.join("skills").join(&skill_name);
    if skill_source.exists() {
        match fs::rename(&skill_source, &target_dir) {
            Ok(_) => {
                let _ = fs::remove_dir_all(&temp_dir);
                println!("Successfully installed skill '{}'.", skill_name);
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&temp_dir);
                println!("Error moving skill: {}", e);
            }
        }
    } else {
        let _ = fs::remove_dir_all(&temp_dir);
        println!("Error: Skill '{}' not found in repository.", skill_name);
    }
}

fn handle_remove(skill_ref: &str) {
    let skill_name = if skill_ref.contains('/') {
        skill_ref.rsplit('/').next().unwrap_or(skill_ref)
    } else {
        skill_ref
    };

    let target_dir = skills_dir().join(skill_name);
    if !target_dir.exists() {
        println!("Skill '{}' is not installed.", skill_name);
        return;
    }

    match fs::remove_dir_all(&target_dir) {
        Ok(_) => println!("Successfully removed skill '{}'.", skill_name),
        Err(e) => println!("Error removing skill: {}", e),
    }
}

fn handle_read_skill_file(args: ReadSkillFileArgs) {
    let skill_name = match args.skill {
        Some(s) => s,
        None => {
            println!("Error: 'skill' is required");
            return;
        }
    };

    let rel_path = match args.path {
        Some(p) => p,
        None => {
            println!("Error: 'path' is required");
            return;
        }
    };

    let skill_dir = skills_dir().join(&skill_name);
    if !skill_dir.exists() {
        println!("Error: Skill '{}' not found", skill_name);
        return;
    }

    // Security: resolve path and check for traversal
    let file_path = skill_dir.join(&rel_path);
    let canonical_skill_dir = match skill_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            println!("Error: Invalid skill directory");
            return;
        }
    };
    let canonical_file_path = match file_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            println!("Error: File not found: {}", rel_path);
            return;
        }
    };

    if !canonical_file_path.starts_with(&canonical_skill_dir) {
        println!("Error: Path traversal not allowed");
        return;
    }

    match fs::read_to_string(&canonical_file_path) {
        Ok(content) => print!("{}", content),
        Err(e) => println!("Error reading file: {}", e),
    }
}

fn handle_run_skill_script(args: RunSkillScriptArgs) {
    let skill_name = match args.skill {
        Some(s) => s,
        None => {
            println!("Error: 'skill' is required");
            return;
        }
    };

    let script_path = match args.script {
        Some(s) => s,
        None => {
            println!("Error: 'script' is required");
            return;
        }
    };

    let script_args = args.args.unwrap_or_default();
    let stdin_input = args.stdin;

    let skill_dir = skills_dir().join(&skill_name);
    if !skill_dir.exists() {
        println!("Error: Skill '{}' not found", skill_name);
        return;
    }

    // Security: resolve path and check for traversal
    let full_path = skill_dir.join(&script_path);
    let canonical_skill_dir = match skill_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            println!("Error: Invalid skill directory");
            return;
        }
    };
    let canonical_script_path = match full_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            println!("Error: Script not found: {}", script_path);
            return;
        }
    };

    if !canonical_script_path.starts_with(&canonical_skill_dir) {
        println!("Error: Path traversal not allowed");
        return;
    }

    // Determine how to run the script
    let is_executable = std::fs::metadata(&canonical_script_path)
        .map(|m| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                m.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                true
            }
        })
        .unwrap_or(false);

    let (program, cmd_args): (String, Vec<String>) = if is_executable {
        (
            canonical_script_path.to_string_lossy().to_string(),
            script_args,
        )
    } else {
        let ext = canonical_script_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        match ext {
            "py" => {
                let mut args = vec![canonical_script_path.to_string_lossy().to_string()];
                args.extend(script_args);
                ("python3".to_string(), args)
            }
            "sh" => {
                let mut args = vec![canonical_script_path.to_string_lossy().to_string()];
                args.extend(script_args);
                ("bash".to_string(), args)
            }
            "js" => {
                let mut args = vec![canonical_script_path.to_string_lossy().to_string()];
                args.extend(script_args);
                ("node".to_string(), args)
            }
            _ => (
                canonical_script_path.to_string_lossy().to_string(),
                script_args,
            ),
        }
    };

    let mut cmd = Command::new(&program);
    cmd.args(&cmd_args).current_dir(&canonical_skill_dir);

    // Handle stdin
    if stdin_input.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    }

    let mut child = match cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            println!("Error executing script: {}", e);
            return;
        }
    };

    // Write stdin if provided
    if let Some(input) = stdin_input {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input.as_bytes());
        }
    }

    // Wait for completion with timeout (2 minutes)
    // Note: Rust's std doesn't have built-in timeout, so we just wait
    match child.wait_with_output() {
        Ok(output) => {
            let mut parts = Vec::new();
            if !output.stdout.is_empty() {
                parts.push(String::from_utf8_lossy(&output.stdout).to_string());
            }
            if !output.stderr.is_empty() {
                parts.push(format!(
                    "[stderr]\n{}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            if !output.status.success() {
                parts.push(format!(
                    "[exit code: {}]",
                    output.status.code().unwrap_or(-1)
                ));
            }
            if parts.is_empty() {
                println!("(no output)");
            } else {
                println!("{}", parts.join("\n"));
            }
        }
        Err(e) => {
            println!("Error executing script: {}", e);
        }
    }
}

fn handle_skill_invocation(tool_name: &str, args: SkillInvocationArgs) {
    if !tool_name.starts_with("skill_") {
        println!("Error: Invalid skill tool name");
        return;
    }

    let skill_name = &tool_name[6..];
    let skill_path = skills_dir().join(skill_name).join("SKILL.md");

    let skill = match parse_skill(&skill_path) {
        Some(s) => s,
        None => {
            println!("Error: Skill '{}' not found or invalid", skill_name);
            return;
        }
    };

    // Build response
    let mut response = format!("# Skill: {}\n\n{}", skill.name, skill.body);

    // Include arguments if provided
    if let Some(arguments) = args.arguments {
        if !arguments.is_empty() {
            response.push_str(&format!("\n\n## Arguments\n{}", arguments));
        }
    }

    // Check for supporting directories
    let skill_dir = skills_dir().join(skill_name);
    let supporting_dirs = ["scripts", "references", "assets"];
    let existing_dirs: Vec<&str> = supporting_dirs
        .iter()
        .filter(|d| skill_dir.join(d).exists())
        .copied()
        .collect();

    if !existing_dirs.is_empty() {
        response.push_str(&format!(
            "\n\n## Supporting Files\nThis skill has supporting files in: {}\n- Use `read_skill_file` to read file contents\n- Use `run_skill_script` to execute scripts",
            existing_dirs.join(", ")
        ));
    }

    println!("{}", response);
}

// ============================================================================
// Tool Call Router
// ============================================================================

fn handle_tool_call(stdin_data: &str) {
    let args_str = stdin_data;
    let tool_name = env::var("CHIBI_TOOL_NAME").unwrap_or_default();

    // Parse args as generic JSON first
    let args_value: serde_json::Value = serde_json::from_str(args_str).unwrap_or_default();

    // Determine tool name if not provided
    let tool_name = if tool_name.is_empty() {
        // Try to infer from args structure
        if args_value.get("action").is_some() {
            "skill_marketplace".to_string()
        } else if args_value.get("script").is_some() && args_value.get("skill").is_some() {
            "run_skill_script".to_string()
        } else if args_value.get("path").is_some() && args_value.get("skill").is_some() {
            "read_skill_file".to_string()
        } else {
            println!("Error: Cannot determine tool name. Please set CHIBI_TOOL_NAME environment variable.");
            return;
        }
    } else {
        tool_name
    };

    // Route to handler
    match tool_name.as_str() {
        "skill_marketplace" => {
            let args: MarketplaceArgs = serde_json::from_value(args_value).unwrap_or_default();
            handle_marketplace(args);
        }
        "read_skill_file" => {
            let args: ReadSkillFileArgs = serde_json::from_value(args_value).unwrap_or_default();
            handle_read_skill_file(args);
        }
        "run_skill_script" => {
            let args: RunSkillScriptArgs = serde_json::from_value(args_value).unwrap_or_default();
            handle_run_skill_script(args);
        }
        name if name.starts_with("skill_") => {
            let args: SkillInvocationArgs = serde_json::from_value(args_value).unwrap_or_default();
            handle_skill_invocation(name, args);
        }
        _ => {
            println!("Error: Unknown tool '{}'", tool_name);
        }
    }
}

// ============================================================================
// CLI Mode
// ============================================================================

fn handle_cli(args: &[String]) {
    if args.len() < 2 {
        println!("Usage: agent-skills <action> [args...]");
        println!("Actions: install, remove, search, list, list_installed");
        return;
    }

    let action = &args[1];
    let marketplace_args = MarketplaceArgs {
        action: Some(action.clone()),
        skill_ref: args.get(2).cloned(),
        query: if action == "search" {
            Some(args[2..].join(" "))
        } else {
            None
        },
    };

    handle_marketplace(marketplace_args);
}

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    // Check for --schema flag
    if args.len() > 1 && args[1] == "--schema" {
        let schema = generate_schema();
        println!("{}", serde_json::to_string(&schema).unwrap());
        return ExitCode::SUCCESS;
    }

    // Check if we're being called as a hook
    if let Ok(hook) = env::var("CHIBI_HOOK") {
        let stdin_data = read_stdin();
        match hook.as_str() {
            "on_start" => handle_on_start_hook(),
            "post_system_prompt" => handle_post_system_prompt_hook(),
            "pre_tool" => handle_pre_tool_hook(&stdin_data),
            _ => println!("{{}}"),
        }
        return ExitCode::SUCCESS;
    }

    // CLI mode (must check before tool call since both read stdin)
    if args.len() > 1 {
        handle_cli(&args);
        return ExitCode::SUCCESS;
    }

    // No CLI args and not a hook — this is a tool call (args via stdin)
    let stdin_data = read_stdin();
    handle_tool_call(&stdin_data);
    ExitCode::SUCCESS
}
