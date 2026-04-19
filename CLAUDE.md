# CLAUDE.md — harmony-glitch

## Session hygiene — NO GIT WORKTREES

**Do not create git worktrees in this repo.** Use `git checkout -b <branch>` in this clone for all branch/PR work. This overrides the `superpowers:using-git-worktrees` skill.

**Do not clean up worktrees either.** Never run `git worktree remove` or `rmdir` on a worktree path — leave stale worktrees for the user to garbage-collect manually.

Why: sessions get anchored to a worktree path, and the Bash tool's CWD reset makes every call fail the moment that path disappears. This has killed multiple sessions mid-cleanup even when the actual merge shipped cleanly. Override only if the user explicitly asks for a worktree or says another agent is working in this repo concurrently.

## Project

Harmony Glitch — a decentralized resurrection of the Glitch MMO on the Harmony network stack.
Currently in Phase A: walking simulator (single-player, no networking).

## Tech Stack

- **Backend:** Rust (Tauri v2)
- **Frontend:** Svelte 5 (runes), TypeScript, PixiJS v8
- **Test:** `cargo test` (Rust, from `src-tauri/`), `npx vitest run` (frontend, from repo root)
- **Build:** `npm run tauri build` (full app), `npm run build` (frontend only)
- **Dev:** `npm run tauri dev`
- **Lint:** `cd src-tauri && cargo clippy`

## Architecture

- Rust owns ALL game logic (physics, collision, state, tick loop)
- PixiJS is a dumb renderer — draws RenderFrames from Rust, no game logic in JS
- Svelte 5 handles UI chrome (menus, overlays, HUD)
- Tauri IPC bridges Rust and frontend (commands + events)

Every line of game logic in Rust is reusable: the same physics/collision code runs
on peers for multiplayer (Phase B) and on harmony-compute WASM nodes (Phase C).

## Svelte 5 Runes

Use `$state()`, `$derived()`, `$derived.by()`, `$props()`, `$effect()`, `onclick={handler}` (NOT `on:click`).

## Coordinate System

Glitch uses Y=0 at bottom (ground level), negative Y goes up.
- Street bounds: `l` (left, negative), `r` (right, positive), `t` (top, negative), `b` (bottom, 0)
- Example: `l=-3000, r=3000, t=-1000, b=0` = 6000px wide, 1000px tall

The renderer must convert to screen coordinates (Y=0 at top, positive Y goes down).

## Parallax

Parallax is computed from layer width relative to middleground width:
- `scroll_factor = layer.w / middleground.w`
- A layer with `w=5460` and middleground `w=6000` scrolls at `5460/6000 = 0.91`

## Key Directories

- `src-tauri/src/street/` — Street data types and XML parser
- `src-tauri/src/physics/` — AABB collision, movement, gravity
- `src-tauri/src/engine/` — Game state, tick loop, RenderFrame
- `src/lib/engine/` — PixiJS rendering (street, avatar, camera)
- `src/lib/components/` — Svelte UI components
- `assets/` — Demo street data and placeholder art

## Design Docs

- `docs/plans/2026-03-07-glitch-resurrection-design.md` — Full design
- `docs/plans/2026-03-07-glitch-phase-a-plan.md` — Phase A implementation plan

## Accessibility

Accessibility is a design REQUIREMENT. Follow WAI-ARIA guidelines. Prefer native HTML
elements over ARIA roles. `role="button"` elements must activate on both Enter and Space.
