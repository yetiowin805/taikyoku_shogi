# Agent instructions

## Git and Cursor environment

- **Do not use workspaces** (no multi-root / workspace moves / `move_agent_to_cloned_root` / cloning the repo into a separate agent workspace). Work only in this repository checkout.
- **Always create a new branch** for new work. Do not continue commits on a branch that already has an open or merged PR unless the user explicitly asks to amend that same PR.
- Prefer branch names like `cursor/<short-topic>` from an updated `main`.
- Commit and push only when the user asks. Open a PR from the new branch when asked.

## Scope

Keep changes focused on the requested task. Do not edit plan files the user attaches unless asked.
