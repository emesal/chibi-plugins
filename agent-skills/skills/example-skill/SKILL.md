---
name: example-skill
description: An example skill that demonstrates the Agent Skills format and capabilities
license: MIT
compatibility: chibi
allowed-tools: Read, Grep, run_skill_script
metadata:
  version: 1.0.0
  author: chibi-plugins
---

# Example Skill

This is an example skill that demonstrates the Agent Skills format. When invoked, you should follow these instructions to help the user.

## Capabilities

This skill provides:
- A simple greeting script in `scripts/hello.py`
- Demonstration of the SKILL.md format

## Instructions

1. When the user asks for help with this example skill, explain what it does
2. You can run the hello script using `run_skill_script` with skill="example-skill" and script="scripts/hello.py"
3. The script accepts an optional name argument

## Example Usage

To greet someone:
- Use `run_skill_script` with `skill="example-skill"`, `script="scripts/hello.py"`, `args=["World"]`

## Notes

This skill restricts which tools can be used while it's active via the `allowed-tools` field.
Only Read, Grep, and run_skill_script are permitted.
