<div align="center">

# AgentSwitch

### 基于插件协议的 AI Agent 配置切换器

[![Version](https://img.shields.io/badge/version-0.1.0-blue)](https://github.com/farion1231/agentswitch/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)](https://github.com/farion1231/agentswitch/releases)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-orange.svg)](https://tauri.app/)

[English](README.md) | 中文 | [更新日志](CHANGELOG.md)

</div>

## 什么是 AgentSwitch？

AgentSwitch 是一款跨平台桌面应用（Windows / macOS / Linux），让你在一个界面中管理和切换多个 AI 编程助手的配置。基于**插件协议**构建——新增一个 Agent 的支持只需编写插件，无需修改核心代码。

**支持的 Agent：**

| Agent | 配置位置 | 说明 |
|-------|---------|------|
| OpenCode | `~/.config/opencode/` | 会话存储在 SQLite |
| OpenClaw | Shell 命令 | 通过外部 CLI 读写 |
| Claude Code | `~/.claude/` | JSON 配置 + settings.json |
| Codex | `~/.codex/` | TOML + auth.json |
| Grok Build | `~/.grok/` | TOML |
| Hermes | `~/.hermes/`（Windows 下默认 `%LOCALAPPDATA%\hermes`） | YAML custom_provider |

## 功能特性

- **一键切换供应商** — 同时切换所有 Agent 的 API 密钥、端点、模型设置
- **从实时配置导入** — 读取现有 Agent 配置并创建可切换的供应商
- **MCP 服务器管理** — 跨所有 Agent 统一管理 Model Context Protocol 服务器
- **Skills 管理** — 发现、安装、同步 `~/.agentswitch/skills/` 到各 Agent
- **Prompt 管理** — 每个 Agent 独立的 Prompt 文件（如 Claude Code 的 `AGENTS.md`、Hermes 的 `SOUL.md`）
- **配置快照** — 保存和恢复完整的 Agent 配置状态
- **数据库备份与恢复** — SQLite WAL 模式，自动轮转备份
- **全局出站代理** — 配置 HTTP/SOCKS5 代理（解决 GFW/国内网络问题）
- **系统托盘** — 无需打开主界面即可快速切换供应商
- **多语言** — 中文、英文、日文、繁体中文

## 架构

```
src-tauri/
├── src/
│   ├── plugin/        # AgentPlugin trait + 6 个内置 native 实现
│   ├── commands/      # Tauri IPC 命令
│   ├── registry.rs    # 插件清单解析、发现、注册
│   ├── services/      # 业务逻辑（mcp、skills、prompts、backup、settings…）
│   └── lib.rs         # 应用入口、invoke_handler、数据库初始化
src/                   # React 前端（Vite + TypeScript）
```

**插件类型：** `native`（Rust，编译进二进制）、`shell`（外部命令）、`ts`（前端 JS 脚本）。

## 快速开始

### 安装

从 [Releases](https://github.com/farion1231/agentswitch/releases/latest) 下载，或从源码构建：

```bash
# 前置要求：Rust 1.85+、Node.js 20+、pnpm
pnpm install
pnpm build          # 或：pnpm build:debug（开发构建）
```

### 开发

```bash
pnpm dev
```

### 常用命令

```bash
pnpm test:unit      # 单元测试
pnpm typecheck      # TypeScript 类型检查
pnpm format:check   # 代码格式检查
cargo test           # Rust 测试（在 src-tauri/ 目录下运行）
cargo clippy         # Rust linter（在 src-tauri/ 目录下运行）
```

## 数据位置

| 项目 | 路径 |
|------|------|
| 数据库 | `~/.agentswitch/agentswitch.db` |
| 设置 | `~/.agentswitch/settings.json` |
| Skills | `~/.agentswitch/skills/` |
| Skills 备份 | `~/.agentswitch/skill-backups/` |
| 备份 | `~/.agentswitch/backups/` |
| 日志 | `~/.agentswitch/logs/` |

## 安全

- **绝不提交密钥** — API 密钥存储在 Agent 原生配置文件中，不在 AgentSwitch 数据库中
- **文件写入验证** — 所有文件操作均有路径遍历保护
- **全局代理** — 可配置出站代理，支持 http/https/socks5/socks5h 协议
- **SQLite WAL 模式** — 原子写入，通过 `rusqlite::backup` API 备份（禁止文件复制）

## 许可证

MIT
