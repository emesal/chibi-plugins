use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Plugin configuration loaded from ~/.chibi/hello_chibi.json
#[derive(Deserialize, Default)]
struct Config {
    /// Path to the chibi binary (required)
    chibi_path: Option<String>,
    /// Path to mcabber's FIFO (default: ~/.mcabber/mcabber.fifo)
    mcabber_fifo: Option<String>,
    /// JID to context mappings (alternative to xmpp-mappings.json)
    mappings: Option<HashMap<String, String>>,
}

/// Inbox entry matching chibi's expected format
#[derive(Serialize, Deserialize)]
struct InboxEntry {
    id: String,
    timestamp: u64,
    from: String,
    to: String,
    content: String,
}

/// Hook data for pre_send_message
#[derive(Deserialize)]
struct SendMessageHookData {
    to: Option<String>,
    content: Option<String>,
    #[allow(dead_code)]
    from: Option<String>,
    #[allow(dead_code)]
    context_name: Option<String>,
}

/// Response indicating message was delivered via hook
#[derive(Serialize)]
struct HookDeliveryResponse {
    delivered: bool,
    via: String,
}

/// Tool call arguments for xmpp_send
#[derive(Deserialize)]
struct XmppSendArgs {
    to: String,
    message: String,
}

fn chibi_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".chibi")
}

fn config_file() -> PathBuf {
    chibi_dir().join("hello_chibi.toml")
}

fn load_config() -> Config {
    if let Ok(content) = fs::read_to_string(config_file()) {
        toml::from_str(&content).unwrap_or_default()
    } else {
        Config::default()
    }
}

fn chibi_path() -> Result<PathBuf, String> {
    let config = load_config();
    match config.chibi_path {
        Some(path) => {
            let path = PathBuf::from(path);
            if path.exists() {
                Ok(path)
            } else {
                Err(format!("chibi binary not found at configured path: {}", path.display()))
            }
        }
        None => Err(format!(
            "chibi_path not configured. Create {} with: chibi_path = \"/path/to/chibi\"",
            config_file().display()
        )),
    }
}

fn mcabber_fifo() -> PathBuf {
    let config = load_config();
    match config.mcabber_fifo {
        Some(path) => PathBuf::from(path),
        None => dirs::home_dir()
            .expect("Could not find home directory")
            .join(".mcabber/mcabber.fifo"),
    }
}

fn mappings_file() -> PathBuf {
    chibi_dir().join("xmpp-mappings.json")
}

/// Load JID -> context mappings from config file or hello_chibi.json
fn load_mappings() -> HashMap<String, String> {
    // First check hello_chibi.json for mappings
    let config = load_config();
    if let Some(mappings) = config.mappings {
        if !mappings.is_empty() {
            return mappings;
        }
    }
    // Fall back to xmpp-mappings.json
    if let Ok(content) = fs::read_to_string(mappings_file()) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    }
}

/// Convert a JID to a context name
fn jid_to_context(jid: &str) -> String {
    let mappings = load_mappings();
    if let Some(ctx) = mappings.get(jid) {
        return ctx.clone();
    }
    // Default: sanitize JID as context name
    jid.replace('@', "_at_").replace('.', "_")
}

/// Send a message to XMPP via mcabber's FIFO
fn send_to_xmpp(jid: &str, message: &str) -> Result<(), String> {
    let fifo = mcabber_fifo();
    if !fifo.exists() {
        return Err(format!(
            "mcabber FIFO not found at {}. Is mcabber running with fifo_name set?",
            fifo.display()
        ));
    }

    // Escape the message for mcabber command
    // mcabber's /say_to expects: /say_to jid message
    // No quoting needed - everything after the JID is the message
    let command = format!("/say_to {} {}\n", jid, message);

    let mut file = OpenOptions::new()
        .write(true)
        .open(&fifo)
        .map_err(|e| format!("Failed to open mcabber FIFO: {}", e))?;

    file.write_all(command.as_bytes())
        .map_err(|e| format!("Failed to write to mcabber FIFO: {}", e))?;

    Ok(())
}

/// Write a message to a context's inbox with proper locking
fn write_to_inbox(context: &str, entry: &InboxEntry) -> Result<(), String> {
    let ctx_dir = chibi_dir().join("contexts").join(context);
    fs::create_dir_all(&ctx_dir)
        .map_err(|e| format!("Failed to create context directory: {}", e))?;

    let inbox_path = ctx_dir.join("inbox.jsonl");
    let lock_path = ctx_dir.join(".inbox.lock");

    // Create/open lock file and acquire exclusive lock
    let lock_file = File::create(&lock_path)
        .map_err(|e| format!("Failed to create lock file: {}", e))?;

    lock_file
        .lock_exclusive()
        .map_err(|e| format!("Failed to acquire inbox lock: {}", e))?;

    // Append entry to inbox
    let mut inbox = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&inbox_path)
        .map_err(|e| format!("Failed to open inbox: {}", e))?;

    let json = serde_json::to_string(entry)
        .map_err(|e| format!("Failed to serialize inbox entry: {}", e))?;

    writeln!(inbox, "{}", json).map_err(|e| format!("Failed to write to inbox: {}", e))?;

    // Lock is released when lock_file is dropped
    Ok(())
}

/// Read all of stdin into a string (args/hook data are passed via stdin)
fn read_stdin() -> String {
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf).unwrap_or_default();
    if buf.is_empty() { "{}".to_string() } else { buf }
}

/// Output the plugin schema (called with --schema)
fn print_schema() {
    let schema = serde_json::json!({
        "name": "xmpp_send",
        "description": "Send a message to an XMPP user or room via mcabber. Messages sent to targets starting with 'xmpp:' will be automatically routed through this plugin.",
        "parameters": {
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "XMPP JID (user@host or room@conference.host)"
                },
                "message": {
                    "type": "string",
                    "description": "Message content to send"
                }
            },
            "required": ["to", "message"]
        },
        "hooks": ["pre_send_message"]
    });
    println!("{}", serde_json::to_string(&schema).unwrap());
}

/// Handle pre_send_message hook - intercept messages to xmpp: targets
fn handle_pre_send_message_hook() -> ExitCode {
    let hook_data = read_stdin();

    let data: SendMessageHookData = match serde_json::from_str(&hook_data) {
        Ok(d) => d,
        Err(_) => {
            println!("{{}}");
            return ExitCode::SUCCESS;
        }
    };

    let target = data.to.unwrap_or_default();
    let content = data.content.unwrap_or_default();

    if let Some(jid) = target.strip_prefix("xmpp:") {
        // This is an XMPP target - intercept and deliver
        match send_to_xmpp(jid, &content) {
            Ok(()) => {
                let response = HookDeliveryResponse {
                    delivered: true,
                    via: format!("xmpp:{}", jid),
                };
                println!("{}", serde_json::to_string(&response).unwrap());
            }
            Err(e) => {
                eprintln!("Failed to send XMPP message: {}", e);
                // Return empty object to let normal delivery proceed as fallback
                println!("{{}}");
            }
        }
    } else {
        // Not an XMPP target - don't intercept
        println!("{{}}");
    }

    ExitCode::SUCCESS
}

/// Handle incoming XMPP message from mcabber eventcmd
/// Called as: hello_chibi MSG IN jid@host /path/to/msgfile
fn handle_eventcmd(args: &[String]) -> ExitCode {
    if args.len() < 4 {
        eprintln!("Usage: hello_chibi MSG IN|OUT|MUC jid [msgfile]");
        return ExitCode::FAILURE;
    }

    let event_type = &args[1]; // MSG
    let direction = &args[2]; // IN, OUT, MUC
    let jid = &args[3];
    let msgfile = args.get(4);

    if event_type != "MSG" {
        // We only handle MSG events
        return ExitCode::SUCCESS;
    }

    // Only process incoming messages
    if direction != "IN" && direction != "MUC" {
        return ExitCode::SUCCESS;
    }

    // Read message content from file if provided
    let message = if let Some(path) = msgfile {
        let path = PathBuf::from(path);
        if path.exists() {
            let mut content = String::new();
            if let Ok(mut f) = File::open(&path) {
                let _ = f.read_to_string(&mut content);
            }
            // Clean up the temp file
            let _ = fs::remove_file(&path);
            content.trim().to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    if message.is_empty() {
        // No message content, nothing to do
        return ExitCode::SUCCESS;
    }

    let context = jid_to_context(jid);

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let entry = InboxEntry {
        id: Uuid::new_v4().to_string(),
        timestamp,
        from: format!("xmpp:{}", jid),
        to: context.clone(),
        content: message,
    };

    if let Err(e) = write_to_inbox(&context, &entry) {
        eprintln!("Failed to write to inbox: {}", e);
        return ExitCode::FAILURE;
    }

    // Get chibi path from config
    let chibi = match chibi_path() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::FAILURE;
        }
    };

    // Trigger chibi to process the inbox
    // Use -S (sub-context) to run in the target context without changing global state
    let prompt = format!(
        "You have received an XMPP message from {}. Check your inbox and reply using xmpp_send(to=\"{}\", message=\"your reply\").",
        jid, jid
    );
    let status = Command::new(&chibi)
        .args(["-S", &context, &prompt])
        .status();

    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            eprintln!("chibi exited with status: {}", s);
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("Failed to run chibi at {}: {}", chibi.display(), e);
            ExitCode::FAILURE
        }
    }
}

/// Handle direct tool call (xmpp_send)
fn handle_tool_call() -> ExitCode {
    let args_json = read_stdin();

    let args: XmppSendArgs = match serde_json::from_str(&args_json) {
        Ok(a) => a,
        Err(e) => {
            println!("Error parsing tool arguments: {}", e);
            return ExitCode::FAILURE;
        }
    };

    match send_to_xmpp(&args.to, &args.message) {
        Ok(()) => {
            println!("Message sent to {} via XMPP", args.to);
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("Failed to send message: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    // Check for --schema flag
    if args.len() > 1 && args[1] == "--schema" {
        print_schema();
        return ExitCode::SUCCESS;
    }

    // Check if we're being called as a hook
    if let Ok(hook) = env::var("CHIBI_HOOK") {
        return match hook.as_str() {
            "pre_send_message" => handle_pre_send_message_hook(),
            _ => {
                // Unknown hook, return empty response
                println!("{{}}");
                ExitCode::SUCCESS
            }
        };
    }

    // Check if we're being called by mcabber eventcmd
    // Format: hello_chibi MSG IN|OUT|MUC jid [msgfile]
    if args.len() >= 4 && args[1] == "MSG" {
        return handle_eventcmd(&args);
    }

    // Otherwise, this is a direct tool call
    handle_tool_call()
}
