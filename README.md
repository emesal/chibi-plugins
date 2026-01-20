# chibi-plugins

Example plugins for [chibi](https://github.com/emesal/chibi). See chibi's [README](https://github.com/emesal/chibi/blob/main/README.md) for more info on the plugin system.

## Structure

Each plugin lives in its own directory:

```
plugins/
├── hello_chibi/      # XMPP bridge (Rust)
├── fetch_url/        # URL fetching (Python)
├── read_context/     # Cross-context inspection (Python)
├── ...
```

## Plugins

| Plugin | Language | Description |
|--------|----------|-------------|
| `hello_chibi` | Rust | XMPP bridge via mcabber - send/receive XMPP messages |
| `bofh_in_the_shell` | Python | BOFH excuse generator |
| `coffee-table` | Python | Coffee tracking |
| `fetch-mcp` | Python | MCP server integration |
| `fetch_url` | Python | Fetch URL content |
| `github-mcp` | Python | GitHub MCP integration |
| `hook-inspector` | Python | Debug hook - logs all hook events |
| `read_context` | Python | Read another context's state |
| `read_file` | Python | Read local files |
| `recurse` | Python | Recursive chibi invocation |
| `run_command` | Python | Execute shell commands |
| `sub-agent` | Python | Spawn sub-agent in separate context |
| `web_search` | Python | Web search |

## Installation

For single-file Python plugins, symlink or copy the script:

```bash
ln -s /path/to/plugins/fetch_url/fetch_url ~/.chibi/plugins/fetch_url
```

For compiled plugins like `hello_chibi`:

```bash
cd hello_chibi
cargo build --release
cp target/release/hello_chibi ~/.chibi/plugins/
```

## Writing plugins

See [chibi's CLAUDE.md](https://github.com/emesal/chibi/blob/main/CLAUDE.md) for the plugin API documentation.
