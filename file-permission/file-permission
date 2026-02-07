#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""
File permission handler for chibi.

Prompts user for y/N confirmation on write_file and patch_file operations.
This is a reference implementation; customise for your own workflows.
"""

import json
import os
import sys

SCHEMA = {
    "name": "file_permission",
    "description": "prompts for permission before file writes",
    "parameters": {"type": "object", "properties": {}, "required": []},
    "hooks": ["pre_file_write"],
}


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--schema":
        print(json.dumps(SCHEMA))
        return

    hook = os.environ.get("CHIBI_HOOK")
    if hook != "pre_file_write":
        # not our hook, auto-approve
        print(json.dumps({"approved": True}))
        return

    hook_data = json.load(sys.stdin)
    tool_name = hook_data.get("tool_name", "unknown")
    path = hook_data.get("path", "unknown")

    # show info to user (stderr)
    if tool_name == "write_file":
        content = hook_data.get("content", "")
        preview = content[:200] + "..." if len(content) > 200 else content
        print(f"\n[{tool_name}] {path}", file=sys.stderr)
        print(f"content preview:\n{preview}\n", file=sys.stderr)
    else:  # patch_file
        find = hook_data.get("find", "")
        replace = hook_data.get("replace", "")
        print(f"\n[{tool_name}] {path}", file=sys.stderr)
        print(f"find: {find[:100]}", file=sys.stderr)
        print(f"replace: {replace[:100]}\n", file=sys.stderr)

    # prompt for permission (stdin is used for hook data, read from tty)
    try:
        with open("/dev/tty") as tty:
            print("allow this file operation? [y/N]: ", end="", flush=True, file=sys.stderr)
            response = tty.readline().strip().lower()
        approved = response == "y"
    except (EOFError, OSError):
        approved = False

    result = {
        "approved": approved,
        "reason": "user approved" if approved else "user denied",
    }
    print(json.dumps(result))


if __name__ == "__main__":
    main()
