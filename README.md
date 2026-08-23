# wez-ai-sidebar

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

`wez-ai-sidebar` is a WezTerm-native monitor for interactive AI coding agents. Agents keep running in ordinary WezTerm panes; the project does not insert tmux, Herdr, or another terminal multiplexer between WezTerm and the agent.

```text
┌ AI AGENTS ─────────┬───────────────────────────────────────┐
│                    │                                       │
│ ● Claude           │  Claude Code                          │
│   TCMT-Portal      │                                       │
│   Shell            │  Allow Bash command?                  │
│                    │                                       │
│ ● OpenCode         │                                       │
│   KiCad            │                                       │
│   WORKING          │                                       │
│                    │                                       │
│ ● Codex            │                                       │
│   firmware         │                                       │
│   IDLE             │                                       │
│                    │                                       │
│ ● Copilot          │                                       │
│   website          │                                       │
│   DONE             │                                       │
│                    │                                       │
│ ● Antigravity      │                                       │
│   backend          │                                       │
│   WORKING          │                                       │
│                    │                                       │
│ ● Kimi             │                                       │
│   research         │                                       │
│   PERMISSION       │                                       │
└────────────────────┴───────────────────────────────────────┘
```

The sidebar is a normal pane that is created at the left edge of every tab. Switching tabs therefore preserves the normal WezTerm tab/pane model while presenting the same daemon snapshot everywhere.

## Status

This repository contains a functional Linux MVP:

- Rust daemon, Unix domain socket, persistent metadata-only cache, and ratatui UI.
- Multiple concurrent instances of the same agent.
- Structured permission detection for Claude Code, OpenCode, Codex CLI, and Copilot CLI.
- Passive Antigravity lifecycle hooks plus a bounded terminal-tail fallback for actual permission prompts.
- Structured Kimi Code CLI lifecycle and permission notification hooks.
- `Enter`/`1-9` focus via `wezterm cli activate-pane`.
- Lua per-tab sidebar creation, toggle, new-tab handling, and optional launch shortcuts.
- Safe JSON integration merging with timestamped backups.
- systemd user service, installer, uninstaller, doctor, tests, and mock states.

No code path approves an agent request. A permission item only takes the user to the relevant pane.

## Install

Install the current GitHub version:

```bash
cargo install --git https://github.com/ryou09269639/wez-ai-sidebar
wez-ai-sidebar install
```

For development from a local checkout:

```bash
cargo install --path .
wez-ai-sidebar install
```

After a future crates.io publication, installation will be:

```bash
cargo install wez-ai-sidebar
wez-ai-sidebar install
```

`install` creates the application config, installs passive agent integrations, writes a WezTerm module and snippet, and installs/enables the systemd user service. Existing JSON is merged, never replaced, and a timestamped backup is written before a modification. Existing WezTerm Lua is not edited because arbitrary Lua ending in `return config` cannot be merged safely.

Add the generated snippet (normally `~/.config/wez-ai-sidebar/wezterm.lua.snippet`) before the final `return config` in your WezTerm config:

```lua
package.path = package.path .. ';/home/YOU/.config/wezterm/?.lua'
local wez_ai = require('wez-ai-sidebar')

wez_ai.setup(config, {
  width = 20,
  position = 'left',
  auto_create = true,
  ctrl_s_toggle = true,
})
```

The module itself is installed at `~/.config/wezterm/wez-ai-sidebar.lua`.

Run diagnostics after installation:

```bash
wez-ai-sidebar doctor
```

Codex requires reviewing/trusting a newly installed non-managed command hook on first use, as required by its current hook security model.

## Commands

```text
wez-ai-sidebar                    Run the sidebar TUI
wez-ai-sidebar daemon             Run the state daemon
wez-ai-sidebar status [--json]    Print the current snapshot
wez-ai-sidebar doctor             Check WezTerm, agents, integrations, daemon
wez-ai-sidebar install            Install config and integrations
wez-ai-sidebar uninstall          Remove owned integration entries/files
```

For UI development, publish a state without starting an agent:

```bash
wez-ai-sidebar mock claude permission-required --id portal
wez-ai-sidebar mock codex working --id firmware
```

## Sidebar keys

```text
j / Down     Next agent
k / Up       Previous agent
Enter        Focus the selected agent pane/tab
1-9          Focus an agent directly
r            Ask the daemon to rescan
q            Close this sidebar pane
?            Toggle help
```

The Lua integration uses `Ctrl+Shift+A` to toggle the sidebar in the active tab. It also adds `Ctrl+S` as a toggle only when neither the user's global keys nor WezTerm's enabled default keys already assign it. Set `ctrl_s_toggle = false` to disable that convenience key. A sidebar closed with `q` stays closed until toggled back on. Optional launch shortcuts are disabled by default to avoid conflicts. Enable them with:

```lua
wez_ai.setup(config, {
  launch_shortcuts = true,
})
```

This adds collision-resistant `Ctrl+Shift+Alt+C` (Claude), `Ctrl+Shift+Alt+O` (OpenCode), and `Ctrl+Shift+Alt+X` (Codex). Supply a custom table instead of `true` to change commands or keys.

## Configuration

`~/.config/wez-ai-sidebar/config.toml`:

```toml
refresh_interval_ms = 300
stale_after_secs = 86400

[sidebar]
width = 20
show_cwd = true
show_message = true
unicode = true

[notifications]
permission = true
done = false

[agents]
claude = true
codex = true
opencode = true
copilot = true
antigravity = true
kimi = true
```

Set `unicode = false` for terminals without reliable Unicode glyph support. Colors use the terminal ANSI palette rather than hard-coded RGB, so they follow the active WezTerm scheme and degrade to the terminal's basic color support.

## Agent support

| Agent | Primary signal | Permission accuracy | Fallback |
|---|---|---:|---|
| Claude Code | `PermissionRequest`, `Notification`, lifecycle hooks | Structured | TTY process/title |
| OpenCode | plugin `permission.asked`, `session.status`, `session.idle` | Structured | TTY process/title |
| OpenAI Codex CLI | `PermissionRequest` and lifecycle hooks | Structured | TTY process/title |
| GitHub Copilot CLI | `notification: permission_prompt`, lifecycle hooks | Structured | TTY process/title |
| Google Antigravity CLI | passive `PostInvocation`/`Stop` hooks | Terminal-tail fallback | TTY process/title |
| Kimi Code CLI | `Notification: permission_prompt`, lifecycle hooks | Structured | TTY process/title |

For Kimi, `wez-ai-sidebar install` appends a marked hook block to `~/.kimi/config.toml` after validating the existing TOML and writing a timestamped backup. Restart Kimi after installation and use Kimi's `/hooks` command to verify the integration. Kimi currently labels its hook API as beta, so releases should keep the parser fixtures aligned with the official schema.

The Antigravity limitation is deliberate. Its documented `PreToolUse` hook runs before permission routing and requires a decision. Returning `allow` would approve the action and returning `ask` would change the user's policy. This project installs neither; it observes passive lifecycle events and scans only the last 30 lines of the known Antigravity pane without persisting the terminal text.

The adapter trait and state model already include `Gemini`, `Aider`, `Cursor`, and `Custom` kinds. Adding another agent means implementing one isolated adapter and an installer fragment; daemon, IPC, state, UI, and WezTerm code do not change.

## Architecture

```text
Claude/Codex/Copilot hooks ─┐
Kimi structured hooks ──────┤
OpenCode plugin events ─────┼─> `wez-ai-sidebar hook` ─┐
Antigravity passive hooks ──┘                          │
                                                      v
                                      Unix socket / spool inbox
                                                      │
WezTerm pane/TTY process discovery + bounded tail ───┤
                                                      v
                                         metadata-only StateStore
                                                      │
                         ┌────────────────────────────┴─────────┐
                         v                                      v
                   sidebar TUI                         `status --json`
                         │
                         └─ `wezterm cli activate-pane --pane-id N`
```

The socket is mode `0600`. Cached JSON contains only agent kind, opaque session ID, cwd/project, status, permission category, short message, pane/tab/window IDs, PID, source, and timestamp. Raw prompts, tool output, transcripts, commands, diffs, API keys, and terminal snapshots are not written by this project.

## WezTerm behavior and limitations

WezTerm has no window-global fixed sidebar. The Lua module therefore creates a top-level left split in each tab using `pane:split`, with `WEZ_AI_SIDEBAR=1` in the spawned environment. The module also checks pane title/process and uses per-tab guards to prevent recursive or duplicate sidebars.

WezTerm does not expose a general “any tab was created” Lua event. The module handles the default new-tab key and new-tab button immediately, then uses `update-status` to lazily cover tabs created through other paths. This is typically visible within one status interval.

`wezterm cli list --format json` provides each pane's TTY. The daemon maps that TTY to exact executable names — via `/proc` on Linux, via `ps -axo pid=,tty=,args=` on macOS — so Codex is found even when its pane title remains `zsh`; prompt text and unrelated command arguments are never used as process evidence. Structured hooks inherit `WEZTERM_PANE` and remain the authoritative source for permission states.

Closing a pane removes all agents mapped to it on the next refresh. Process-discovered agents are also removed when their process exits, and structured `SessionEnd` events remove the corresponding session immediately. Remaining agents are ordered with permission/input waits first, then working and idle sessions; terminal error/done states stay at the bottom.

## Desktop notifications

On Linux, transitions into `PermissionRequired` optionally call `notify-send`. Notifications are transition-based, so repeated refreshes do not spam. Disable them in config if no notification daemon is available.

## Troubleshooting

### Sidebar says “No agents”

```bash
wez-ai-sidebar doctor
wez-ai-sidebar status --json
systemctl --user status wez-ai-sidebar
journalctl --user -u wez-ai-sidebar -n 100
```

Restart an agent after installing hooks; most CLIs load hook configuration at startup. OpenCode loads global plugins at startup as well.

### Enter does not focus an agent

Run `wezterm cli list --format json` in the same WezTerm GUI. If multiple independent GUI instances are running, start the sidebar inside the intended instance so it inherits that GUI's `WEZTERM_UNIX_SOCKET`.

### A sidebar is duplicated

Check that the sidebar binary was not wrapped in another shell script that changes its title/process name, and verify the spawned pane has `WEZ_AI_SIDEBAR=1`. Toggle once with `Ctrl+Shift+A` to normalize the active tab.

### Antigravity misses an approval

The terminal fallback intentionally uses conservative English patterns and the last 30 lines only. Add a parser fixture and pattern in `src/adapters/common.rs`; do not solve this by returning `allow` from an Antigravity hook.

### Remove the project

```bash
wez-ai-sidebar uninstall
```

This removes only entries/files containing the project marker. It preserves `config.toml`, cached state, unrelated agent hooks, and unrelated WezTerm configuration.

## Research sources

The integration choices were checked against current first-party documentation on 2026-08-21:

- [WezTerm CLI list JSON](https://wezterm.org/cli/cli/list.html), [split pane](https://wezterm.org/config/lua/pane/split.html), and [new-tab button event](https://wezterm.org/config/lua/window-events/new-tab-button-click.html)
- [Claude Code hooks reference](https://code.claude.com/docs/en/hooks)
- [OpenCode plugins/events](https://opencode.ai/docs/plugins/) and [server architecture](https://opencode.ai/docs/server/)
- [OpenAI Codex hooks](https://learn.chatgpt.com/codex/hooks) and [App Server approval protocol](https://developers.openai.com/codex/app-server)
- [GitHub Copilot hooks reference](https://docs.github.com/en/copilot/reference/hooks-reference)
- [Google Antigravity hooks](https://antigravity.google/docs/hooks)
- [Kimi Code CLI hooks](https://github.com/MoonshotAI/kimi-cli/blob/main/docs/en/customization/hooks.md)

See [docs/research.md](docs/research.md) for the decisions and fallback rationale.

## Development

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
cargo build --release
```

The test suite covers structured event parsers, output patterns, state transitions, stale removal, multiple agent instances, JSON integration merging, and WezTerm pane mapping.

## License

MIT
