---
title: Finding your games
description: Installed versus All games, filtering and sorting the library, and fixing a wrong match.
sidebar:
  order: 1
---

## Installed and All games

**Installed**, the default, is games with files on this PC.

**All games** adds everything else in your Steam library, usually about ten times as many.

:::note
**With Steam running you get the fuller list.** Griddle asks Steam directly, which knows about
games you own but have never launched on this PC. With Steam closed it falls back to what Steam's
files remember, which is a few hundred games short. Nothing else changes, and you do not have to
do anything.
:::


## Filter and sort

The filter box narrows by name as you type. **Sort** offers name, recently played, or most played.
Both stay pinned as you scroll.

**Size −/+** at the top right makes the capsules bigger or smaller. Your choice is remembered, and
the [artwork tabs](/griddle/using/artwork-types/) have the same control with their own settings.

## When Griddle matches the wrong game

Griddle finds a game on SteamGridDB by its Steam app ID, then falls back to searching by name if
that fails. The fallback is a guess, and sometimes a wrong one. It happens most with re-releases
and remasters, and always with non-Steam shortcuts, whose IDs SteamGridDB has never seen.

To check, expand **Filters** above the results. The button on the right names the SteamGridDB game
your artwork is coming from, or reads **Wrong game?** if nothing matched. Click it to search and
pick the right one, and Griddle remembers your choice.

:::note
The heading at the top of the screen always shows *your* Steam game's name, whatever SteamGridDB
matched, so a wrong match never shows up there.
:::
