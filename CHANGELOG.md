# Changelog

All notable changes to AgentSwitch will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.1.0] - 2026-09-18

### Added

- **Plugin-based architecture** (`AgentPlugin` trait) — core depends on abstractions; new agents = write plugin + register
- **6 builtin native plugins**: OpenCode, OpenClaw, Claude Code, Codex, Grok Build, Hermes
- **Provider management** — one-click switch API keys, endpoints, model settings across all agents
- **Import from live configs** — reads real agent configs and creates switchable providers
- **MCP server management** — unified panel for Model Context Protocol servers across all agents
- **Skills management** — discover, install, and sync skills to each agent; SSOT at `~/.agentswitch/skills/`
- **Prompt management** — per-agent prompt files with互斥 active promotion
- **Profile snapshots** — save and restore full agent configuration states
- **Database backup & restore** — SQLite WAL-mode with automatic rotation
- **Global outbound proxy** — HTTP/SOCKS5/SOCKS5H support for all network requests
- **System tray** — quick switch providers from the menu bar
- **Settings panel** — tabbed interface with theme, language, and advanced options
- **i18n** — Chinese, English, Japanese, Traditional Chinese
- **Renderer crash recovery** — error boundary with on-disk reports and reload screen
- **Diagnostic logs** — persistent, size-rotated, never record secrets
- **Release CI/CD** — 5-platform cross-compile (Windows x86_64/ARM64, Ubuntu x86_64/ARM64, macOS), macOS notarization, R2 CDN sync
- **TS plugin support** — load external TypeScript plugins via `new Function` execution

### Removed

- **Pricing feature** — models.dev sync removed (7,838 entries causing UI lag; data quality unreliable)
- **Cost display** — usage panel now shows tokens only
- **Provider presets** — commercial presets with affiliate links removed; MCP presets retained
- **Proxy全家桶** — per-agent proxy configuration removed in favor of global outbound proxy
- **v2/ subdirectory** — project root now directly contains the codebase

### Security

- Path traversal validation on all file operations (`host_write_file`, `host_list_files`)
- Proxy test uses isolated temp client instead of global client
- `host_write_file` validates directory boundary before `create_dir_all`

## [Unreleased]

### Planned

- Deep Link support (`agentswitch://`)
- Cloud sync
- Balance/subscription display
- Gemini and Claude Desktop plugin化
