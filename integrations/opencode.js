// Installed by wez-ai-sidebar. OpenCode loads global plugins at startup.
export const WezAiSidebar = async ({ directory }) => ({
  event: async ({ event }) => {
    const relevant = new Set([
      "permission.asked",
      "permission.replied",
      "session.created",
      "session.status",
      "session.idle",
      "session.error",
      "session.deleted",
    ]);
    if (!relevant.has(event.type)) return;
    const payload = { ...event, cwd: directory };
    // Spawn with an argv array (no shell) so event data cannot become a command.
    // OpenCode's bundled Bun runtime rejects top-level stdout/stderr options
    // (throws "stdio must be an array..."); the stdio tuple form is required.
    Bun.spawnSync(["wez-ai-sidebar", "hook", "opencode"], {
      stdin: JSON.stringify(payload),
      stdio: ["pipe", "ignore", "ignore"],
    });
  },
});
