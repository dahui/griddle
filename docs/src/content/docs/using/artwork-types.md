---
title: The five artwork types
description: What Steam's capsule, wide capsule, hero, logo and icon slots are, and where each one appears.
sidebar:
  order: 2
---

Steam shows a game through five separate images. Griddle gives each one a tab.

| Tab | Where you see it | Shape |
|---|---|---|
| **Capsule** | The library grid — the tall tile you scroll past | Portrait, 600×900 |
| **Wide Capsule** | The horizontal shelves: recent games, collections, search | Landscape, 920×430 |
| **Hero** | The wide banner across the top of a game's page | Very wide, 1920×620 |
| **Logo** | The game's name, laid over the hero banner | Transparent PNG |
| **Icon** | Small icon in lists, the taskbar, and desktop shortcuts | Square |

A game can have any combination of them. Changing one never affects the others.

## Capsule and Wide Capsule come from the same place

Both are "grids" on SteamGridDB, separated only by shape. That matters when you use
[filters](/griddle/using/filters/): a size that makes sense for one is meaningless for the other,
and Griddle narrows your filter set to whatever the current tab can actually use.

## Logos have a position

A logo is drawn on top of the hero banner, so Steam also stores *where*. When you apply a logo to
a game that has never had one, Griddle writes a sensible default position alongside it — bottom
left, at half size. Without that, some games render no logo at all.

## Icons are the exception

Icons are the one slot Steam's live-apply cannot set, and Griddle is honest about it rather than
offering a control that quietly does nothing:

- **Non-Steam shortcuts** — icons work, but they require Steam to be closed. See
  [Non-Steam shortcuts](/griddle/notes/non-steam-shortcuts/).
- **Steam games** — the Icon tab shows the current icon but cannot change it. There is no route
  Steam accepts.

## Animated artwork

SteamGridDB has animated capsules and heroes, and they work — Steam plays them in both the desktop
library and Big Picture. Griddle marks them in the browser so you can tell before you apply one.
