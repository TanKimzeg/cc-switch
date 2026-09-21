<div align="center">

# AgentSwitch

### Plugin-based AI Agent Configuration Switcher

[![Version](https://img.shields.io/badge/version-0.1.0-blue)](https://github.com/farion1231/agentswitch/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)](https://github.com/farion1231/agentswitch/releases)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-orange.svg)](https://tauri.app/)

English | [中文](README_ZH.md) | [Changelog](CHANGELOG.md)

</div>

## What is AgentSwitch?

AgentSwitch is a cross-platform desktop app (Windows / macOS / Linux) that lets you manage and switch configurations for multiple AI coding agents from a single UI. Built on a **plugin protocol** — adding support for a new agent means writing a plugin, not modifying core code.

**Supported agents:**

| Agent | Config Location | Notes |
|-------|----------------|-------|
| OpenCode | `~/.config/opencode/` | Sessions stored in SQLite |
| OpenClaw | Shell command | Read/write via external CLI |
| Claude Code | `~/.claude/` | JSON config + settings.json |
| Codex | `~/.codex/` | TOML + auth.json |
| Grok Build | `~/.grok/` | TOML |
| Hermes | `~/.hermes/` (or `%LOCALAPPDATA%\hermes` on Windows) | YAML custom_provider |

## Features

- **One-click provider switching** — change API keys, endpoints, model settings across all agents simultaneously
- **Import from live configs** — reads real agent configs and creates switchable providers
- **MCP server management** — unified panel for Model Context Protocol servers across all agents
- **Skills management** — discover, install, and sync skills from `~/.agentswitch/skills/` to each agent
- **Prompt management** — per-agent prompt files (e.g. Claude Code's `AGENTS.md`, Hermes' `SOUL.md`)
- **Profile snapshots** — save and restore full agent configuration states
- **Database backup & restore** — SQLite WAL-mode with automatic backup rotation
- **Global outbound proxy** — configure HTTP/SOCKS5 proxy for all network requests (solves GFW/China network issues)
- **System tray** — quick switch providers without opening the full app
- **i18n** — Chinese, English, Japanese, Traditional Chinese

## Architecture

```
src-tauri/
├── src/
│   ├── plugin/        # AgentPlugin trait + 6 builtin native implementations
│   ├── commands/      # Tauri IPC commands
│   ├── registry.rs    # Plugin manifest parsing, discovery, registration
│   ├── services/      # Domain logic (mcp, skills, prompts, backup, settings…)
│   └── lib.rs         # App entry, invoke_handler, DB init
src/                   # React frontend (Vite + TypeScript)
```

**Plugin types:** `native` (Rust, compiled into binary), `shell` (external command), `ts` (frontend JS scripts).

## Getting Started

### Install

Download from [Releases](https://github.com/farion1231/agentswitch/releases/latest) or build from source:

```bash
# Prerequisites: Rust 1.85+, Node.js 20+, pnpm
pnpm install
pnpm build          # or: pnpm build:debug for dev build
```

### Development

```bash
pnpm dev
```

### Commands

```bash
pnpm test:unit      # Run unit tests
pnpm typecheck      # TypeScript type checking
pnpm format:check   # Code formatting check
cargo test           # Rust tests (run from src-tauri/)
cargo clippy         # Rust linter (run from src-tauri/)
```

## Data Location

| Item | Path |
|------|------|
| Database | `~/.agentswitch/agentswitch.db` |
| Settings | `~/.agentswitch/settings.json` |
| Skills | `~/.agentswitch/skills/` |
| Skill Backups | `~/.agentswitch/skill-backups/` |
| Backups | `~/.agentswitch/backups/` |
| Logs | `~/.agentswitch/logs/` |

## Security

- **Never commit secrets** — API keys are stored in agent-native config files, not in AgentSwitch's database
- **File write validation** — path traversal protection on all file operations
- **Global proxy** — configurable outbound proxy with support for http/https/socks5/socks5h protocols
- **SQLite WAL mode** — atomic writes, backup via `rusqlite::backup` API (no file-copy)

## License

MIT
