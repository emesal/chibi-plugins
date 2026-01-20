# hello_chibi

XMPP bridge plugin for chibi. Enables contexts to send and receive XMPP messages via [mcabber](https://mcabber.com/).

## How it works

```
XMPP network
     ↕
mcabber (XMPP client, runs in tmux/screen)
     ↕ eventcmd / FIFO
hello_chibi plugin
     ↕ inbox / pre_send_message hook
chibi contexts
```

**Outgoing messages**: When a context uses `send_message(to="xmpp:user@host", ...)`, the `pre_send_message` hook intercepts it and sends via mcabber's FIFO.

**Incoming messages**: mcabber's eventcmd calls this plugin when messages arrive. The plugin writes to the target context's inbox and triggers chibi.

## Setup

### 1. Build the plugin

```bash
cd hello_chibi
cargo build --release
```

### 2. Install to chibi plugins directory

```bash
cp target/release/hello_chibi ~/.chibi/plugins/
```

### 3. Configure hello_chibi

Create `~/.chibi/hello_chibi.toml`:

```toml
chibi_path = "/home/youruser/projects/chibi/target/release/chibi"
mcabber_fifo = "/home/youruser/.mcabber/mcabber.fifo"

[mappings]
"alice@example.org" = "alice-chat"
"bob@example.org" = "bob-assistant"
```

| Field | Required | Description |
|-------|----------|-------------|
| `chibi_path` | **Yes** | Absolute path to the chibi binary |
| `mcabber_fifo` | No | Path to mcabber's FIFO (default: `~/.mcabber/mcabber.fifo`) |
| `mappings` | No | JID to context name mappings |

If no mapping exists for a JID, it's sanitized to a context name (e.g., `alice@example.org` becomes `alice_at_example_org`).

### 4. Configure mcabber

Add to `~/.mcabber/mcabberrc`:

```
set jid = yourbot@yourserver.org
set password = your-password
set server = yourserver.org

set events_command = ~/.chibi/plugins/hello_chibi
set event_log_files = 1
set event_log_dir = /tmp/mcabber-events
set fifo_name = ~/.mcabber/mcabber.fifo
```

Create the event log directory:

```bash
mkdir -p /tmp/mcabber-events
```

### 5. Run mcabber as a systemd service

Create a systemd user service file at `~/.config/systemd/user/mcabber.service`:

```ini
[Unit]
Description=mcabber XMPP client for chibi
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStartPre=/bin/mkdir -p %h/.mcabber /tmp/mcabber-events
ExecStartPre=/bin/bash -c 'test -p %h/.mcabber/mcabber.fifo || mkfifo %h/.mcabber/mcabber.fifo'
# mcabber is a TUI app - use 'script' to provide a pseudo-terminal
ExecStart=/usr/bin/script -q -c /usr/bin/mcabber /dev/null
Restart=on-failure
RestartSec=10

Environment=HOME=%h
Environment=TERM=dumb

[Install]
WantedBy=default.target
```

Note: mcabber is a TUI application. The `script` command provides the pseudo-terminal it needs to run headlessly.

Enable and start the service:

```bash
# Create the directory if needed
mkdir -p ~/.config/systemd/user

# Reload systemd to pick up the new service
systemctl --user daemon-reload

# Enable the service to start on login
systemctl --user enable mcabber

# Start the service now
systemctl --user start mcabber

# Check status
systemctl --user status mcabber

# View logs
journalctl --user -u mcabber -f
```

To ensure the service runs even when you're not logged in:

```bash
sudo loginctl enable-linger $USER
```

**Alternative: tmux**

If you prefer tmux over systemd:

```bash
tmux new-session -d -s mcabber 'mcabber'
```

## Usage

### Sending XMPP messages from a context

The LLM can send messages using the `xmpp:` prefix with `send_message`:

```
send_message(to="xmpp:alice@example.org", content="Hello from chibi!")
```

Or call the tool directly:

```
xmpp_send(to="alice@example.org", message="Hello!")
```

### Receiving XMPP messages

When someone sends an XMPP message to your mcabber account:

1. mcabber triggers the eventcmd
2. hello_chibi writes to the appropriate context's inbox
3. chibi is invoked to process the inbox
4. The LLM sees the message and can respond

## Modes of operation

The plugin handles three different invocation patterns:

| Mode | Trigger | Purpose |
|------|---------|---------|
| `--schema` | chibi plugin discovery | Returns tool schema and hook registration |
| `CHIBI_HOOK=pre_send_message` | chibi hook system | Intercepts `xmpp:` targets |
| `MSG IN jid [file]` | mcabber eventcmd | Processes incoming XMPP messages |
| `CHIBI_TOOL_ARGS={...}` | Direct tool call | LLM calls `xmpp_send` directly |

## Security notes

- Message content is read from temp files created by mcabber, then deleted
- The FIFO is a local Unix socket, not network-exposed
- JIDs are validated/sanitized before use as context names
- Inbox writes use file locking to prevent race conditions
