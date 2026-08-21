# Integration research and architecture decision record

Research date: 2026-08-21 (Asia/Tokyo)

## Decision

Use one passive event ingest command for all structured integrations, a metadata-only daemon as the source of truth, and a WezTerm-native pane for presentation. Detection priority is:

1. Structured event/plugin event
2. Lifecycle/notification hook
3. Session file (adapter extension point; none is persisted or depended on in the MVP)
4. Bounded terminal tail
5. Process/title presence

Higher-quality sources are not overwritten by lower-quality sources. Terminal scraping is currently restricted to Antigravity and process-only placeholders.

## WezTerm

Official behavior:

- [`wezterm cli list --format json`](https://wezterm.org/cli/cli/list.html) returns window, tab, pane, workspace, dimensions, title, and cwd.
- [`pane:split`](https://wezterm.org/config/lua/pane/split.html) supports `direction`, top-level tab splitting, a cell/fraction size, arguments, and per-command environment variables.
- [`update-status`](https://wezterm.org/config/lua/window-events/update-status.html) is periodic and serialized per window.
- [`new-tab-button-click`](https://wezterm.org/config/lua/window-events/new-tab-button-click.html) covers the tab-bar button, but the Lua event list has no general event for every tab creation path.
- [`wezterm cli activate-pane`](https://wezterm.org/cli/cli/activate-pane.html) activates the target pane and its containing tab.

Consequences:

- A sidebar must be duplicated per tab.
- New-tab key/button creation is immediate; other tab paths are reconciled during status updates.
- `WEZTERM_PANE`, inherited by agent hook commands, is a stronger mapping key than cwd or title.

## Claude Code

The [official hooks reference](https://code.claude.com/docs/en/hooks) documents:

- `PermissionRequest` immediately before the permission dialog, including `session_id`, `cwd`, `tool_name`, `tool_input`, and suggestions.
- `Notification` types `permission_prompt`, `idle_prompt`, and `elicitation_dialog`.
- `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `StopFailure`, `SessionStart`, and `SessionEnd` lifecycle points.

Decision: use `PermissionRequest` as authoritative permission state, classify by tool name, and return `{}` so Claude's normal prompt remains in control.

## OpenCode

The [official plugin documentation](https://opencode.ai/docs/plugins/) lists `permission.asked`, `permission.replied`, `session.status`, `session.idle`, `session.error`, and message/tool events. Global plugins are loaded from `~/.config/opencode/plugins/`. The [server documentation](https://opencode.ai/docs/server/) confirms the TUI is a client of an HTTP server and that events are available as SSE, but connecting to the random TUI port would add discovery and authentication complexity.

Decision: install a minimal local JavaScript plugin that forwards only event objects to the Rust parser. No prompt or tool body is persisted.

## OpenAI Codex CLI

The [official OpenAI Codex hooks guide](https://learn.chatgpt.com/codex/hooks) documents current release behavior for `PermissionRequest`, including `session_id`, `turn_id`, `cwd`, `tool_name`, and `tool_input`. It also documents lifecycle hooks and the trust review required for non-managed command hooks. The [App Server protocol](https://developers.openai.com/codex/app-server) is even more structured and has server-initiated command/file/network approval requests, but using it would replace the normal Codex TUI as the client, violating the desired “agent runs normally in its pane” UX.

Decision: use normal Codex TUI plus lifecycle hooks. Return `{}` from `PermissionRequest`, which declines to decide and preserves the native approval flow.

## GitHub Copilot CLI

The [official GitHub hook reference](https://docs.github.com/en/copilot/reference/hooks-reference) documents `permissionRequest` and asynchronous `notification`. Crucially, `permissionRequest` occurs before permission rules/session approvals, while `notification_type=permission_prompt` is emitted when the CLI actually prompts. It also exposes `elicitation_dialog`, lifecycle stop, and error events. User hooks live below `~/.copilot/hooks/`.

Decision: treat `notification: permission_prompt` as the authoritative human-attention signal. Do not install an approving/denying decision.

## Google Antigravity CLI

The [official Antigravity hooks documentation](https://antigravity.google/docs/hooks) currently exposes `PreToolUse`, `PostToolUse`, `PreInvocation`, `PostInvocation`, and `Stop`. `PreToolUse` requires a decision among `allow`, `deny`, `ask`, `force_ask`, and `deny_unless_prior_grant`. There is no documented post-routing permission-prompt event.

Decision:

- Do not install `PreToolUse`: `allow` would violate the no-auto-approval rule, while `ask`/`force_ask` changes the user's permission policy and cannot tell whether a prompt would otherwise appear.
- Install passive `PostInvocation` and `Stop` observers.
- For a known Antigravity pane only, read the final 30 terminal lines and apply conservative permission/input patterns in memory. Never write that terminal text to disk.

This is a documented best-effort integration until Google exposes a passive prompt event.

## Kimi Code CLI

The [official Kimi hooks documentation](https://github.com/MoonshotAI/kimi-cli/blob/main/docs/en/customization/hooks.md) currently documents 13 lifecycle events. Hook commands receive structured JSON on stdin with common `session_id`, `cwd`, and `hook_event_name` fields. The `Notification` event includes `notification_type`, `title`, `body`, and `severity`; the official permission-notification example uses `notification_type=permission_prompt`.

Decision: install marked `[[hooks]]` entries in `~/.kimi/config.toml`. Use `Notification: permission_prompt` as the authoritative human-attention signal and normal lifecycle events for working/done/error. Kimi hook stdout is left empty so the observer does not add content to Kimi's context or influence approval. The installer parses the existing TOML, preserves unrelated text and comments, writes a timestamped backup, and removes only its marked block during uninstall.

## Existing projects

Projects reviewed for UX/architecture comparison:

- [Herdr](https://github.com/SuperCodeAgents/herdr-terminal) demonstrates a useful blocked/working/done sidebar but is itself a multiplexer, so it is not used as the host UI.
- [tmux-agent-sidebar](https://github.com/hiroppy/tmux-agent-sidebar) demonstrates hook-first semantic state plus local plugins, but depends on tmux.
- [wez-sidebar](https://github.com/kok1eee/wez-sidebar) validates the WezTerm pane + Claude hook approach, but its published scope is Claude-oriented and manual per-pane creation.
- [TmuxCC](https://github.com/nyanko3141592/tmuxcc) validates direct keyboard navigation among multiple agents, but depends on tmux.

No source code was copied. The MVP adopts the general semantic-event principle while preserving WezTerm's native tab/pane ownership.

## Security boundary

- Hook stdout never returns `allow` or another approval grant.
- IPC is a local Unix socket with mode `0600`.
- State messages are whitespace-normalized and capped at 120 characters.
- Structured input is parsed in memory; raw stdin is not logged.
- Terminal fallback text is bounded and never persisted.
- Installer merges JSON and writes a backup before modifying an existing file.
- The OpenCode plugin launches argv directly, without a shell.
