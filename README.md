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
├── run_command/      # Shell command execution (bash)
├── ...
```

## Plugins

| Plugin | Language | Description |
|--------|----------|-------------|
| `agent-skills` | Rust | Agent Skills marketplace - install/invoke skills from SKILL.md |
| `hello_chibi` | Rust | XMPP bridge via mcabber - send/receive XMPP messages |
| `bofh_in_the_shell` | bash | Execute shell commands without guardrails (joke plugin) |
| `coffee-table` | Python | Shared inter-context communication space |
| `fetch-mcp` | bash | MCP server wrapper for URL fetching |
| `fetch_url` | bash | Fetch URL content via curl |
| `file-permission` | Python | Prompts for user confirmation on file writes (hook) |
| `github-mcp` | Python | GitHub MCP integration with tool caching |
| `hook-inspector` | bash | Debug hook - logs all hook events to file |
| `read_context` | bash | Read another context's state (read-only) |
| `read_file` | bash | Read local files |
| `recurse` | bash | Signal chibi to continue processing |
| `run_command` | bash | Execute shell commands with confirmation |
| `sub-agent` | bash | Spawn sub-agent in separate context |
| `web_search` | Python | Web search via DuckDuckGo |

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
