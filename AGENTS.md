# Agent instructions

## NEVER use Cursor workspaces

**Do not** call or rely on workspace / root-move MCP tools in this project. They hang or strand the session.

Forbidden (non-exhaustive):

- `move_agent_to_cloned_root`
- `move_agent_to_root` (unless the user explicitly asks to retarget an already-open folder)
- Creating or switching into a cloned / separate agent workspace or git worktree “for the agent”
- Multi-root workspace setups for this repo

**Work only in this repository checkout:** `/home/frank/taikyoku_shogi` (or whatever path the user’s open folder already is). Use normal `git checkout -b` in-place.

Why this exists: workspace MCP moves have repeatedly hung or done nothing useful here; the earlier `AGENTS.md` on `cursor/search-speed-next` never merged to `main`, so agents kept ignoring the rule.

## Git branches

- **Always create a new branch** for new work from updated `main` (e.g. `cursor/<short-topic>`).
- Do **not** pile new features onto a branch that already has an open or merged PR unless the user explicitly asks to amend that same PR.
- Commit and push only when the user asks. Open a PR from the new branch when asked.

## Scope

Keep changes focused on the requested task. Do not edit plan files the user attaches unless asked.
