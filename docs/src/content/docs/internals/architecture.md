---
title: Architecture
description: How Griddle is put together, from crates and packages to where decisions live.
sidebar:
  order: 1
---

Griddle is a Tauri application: a Rust backend and a React frontend in one Windows executable.

## The pieces

| | What it is |
|---|---|
| `crates/griddle-core` | All the logic. No Tauri, no `anyhow`. Reads Steam's files, talks to SteamGridDB, writes artwork, reads the controller. |
| `crates/griddle-app` | The desktop shell. Thin: it exposes commands and owns nothing. |
| `packages/shared` | Logic the UI needs and the backend also has: filter vocabularies, logo maths, the focus grid. |
| `apps/desktop` | The React interface. |
| `docs/` | This site. Deliberately outside the workspace, so its dependencies never touch the app's. |

The split is enforced rather than encouraged: a CI check fails if `griddle-core` grows a Tauri or
`anyhow` dependency.

## Where decisions live

Two rules shape most of the code.

**Nothing fails silently.** Ignoring a `Result` does not compile. Neither does `unwrap()` or
`expect()`. The failure this guards against is corrupting a Steam configuration halfway through
writing it.

**Only four modules may write files:** the artwork store, the shortcuts writer, settings, and the
cache. Every other file write in the repository fails CI unless it carries an explicit annotation.
See [The write boundary](/griddle/internals/the-write-boundary/).

## The interface layer

The frontend never decides anything. Commands return a typed error carrying a *kind*, which keeps
"Steam is running, close it" distinguishable from "the network timed out", plus an action telling
the user what to do about it.

Applying artwork is a ladder: try live, fall back to writing a file. Falling back is not an error.
The result says which path ran, so the interface can say whether a restart is needed.

## Further reading

The full engineering record, including everything measured about Steam's file formats and every
wrong turn taken, is in `CLAUDE.md` at the repository root. It is written for whoever maintains
the code rather than for whoever uses the app.
