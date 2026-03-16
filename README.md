# chibi-plugins

Example plugins for [chibi](https://github.com/emesal/chibi). See chibi's [README](https://github.com/emesal/chibi/blob/main/README.md) for more info on the plugin system.

## Structure

Each plugin lives in its own directory:

```
plugins/
├── agent-skills/     # Agent Skills marketplace (Rust)
├── hello_chibi/      # XMPP bridge (Rust)
├── file-permission/  # File write confirmation (Python)
├── web_search/       # Web search (Python)
├── ...
```

## Plugins

| Plugin | Language | Description |
|--------|----------|-------------|
| `agent-skills` | Rust | Agent Skills marketplace - install/invoke skills from SKILL.md |
| `bofh_in_the_shell` | bash | Execute shell commands without guardrails (joke plugin) |
| `coffee-table` | Python | Shared inter-context communication space |
| `file-permission` | Python | Prompts for user confirmation on file writes (hook) |
| `hello_chibi` | Rust | XMPP bridge via mcabber - send/receive XMPP messages |
| `hook-inspector` | bash | Debug hook - logs all hook events to file |
| `lang_rust` | Rust | Language plugin: extracts symbols/refs from Rust source files (tree-sitter) |
| `web_search` | Python | Web search via DuckDuckGo |

### Removed (now builtins)

The following plugins have been superseded by built-in tools in chibi-core:

- `fetch_url` → `fetch_url` builtin
- `read_context` → `read_context` builtin
- `read_file` → `file_head`/`file_lines` builtins
- `run_command` → `shell_exec` builtin + `pre_shell_exec` hook
- `recurse` → `call_agent` builtin
- `fetch-mcp` → `fetch_url` builtin
- `github-mcp` → `gh` CLI via `shell_exec`

## Plugin convention

Plugins receive arguments via **stdin** as JSON:

- **Tools**: chibi pipes the tool arguments JSON to stdin.
- **Hooks**: chibi pipes the hook data JSON to stdin. The `CHIBI_HOOK` env var identifies which hook is firing.
- **Schema**: called with `--schema` as the first argument; must print JSON schema to stdout.

```python
# Python example
if len(sys.argv) > 1 and sys.argv[1] == "--schema":
    print(json.dumps({...}))
    sys.exit(0)

if os.environ.get("CHIBI_HOOK"):
    data = json.load(sys.stdin)  # hook data via stdin
    print("{}")
    sys.exit(0)

params = json.load(sys.stdin)  # tool args via stdin
print("result")
```

```bash
# bash example
if [[ "$1" == "--schema" ]]; then
    cat <<'EOF'
{ ... }
EOF
    exit 0
fi

# Read args from stdin
ARGS=$(cat /dev/stdin)
value=$(echo "$ARGS" | jq -r '.key')
```

Plugins that need interactive user input (e.g. confirmation prompts) should read from `/dev/tty` since stdin is used for JSON args.

## Installation

For single-file plugins, symlink or copy the script:

```bash
ln -s /path/to/plugins/web_search/web_search ~/.chibi/plugins/web_search
```

For compiled plugins like `hello_chibi` or `agent-skills`:

```bash
cd hello_chibi
cargo build --release
cp target/release/hello_chibi ~/.chibi/plugins/
```

## Writing plugins

See [chibi's](https://github.com/emesal/chibi/) documentation on [plugins](https://github.com/emesal/chibi/blob/main/docs/plugins.md) and [hooks](https://github.com/emesal/chibi/blob/main/docs/hooks.md) for more information.
