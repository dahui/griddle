---
title: Troubleshooting
description: Steam not found, live apply unavailable, missing results, wrong matches, and SmartScreen.
sidebar:
  order: 3
---

**Settings → Diagnostics** reports what Griddle found on your machine — Steam's location, your
account, whether instant apply is available. Almost every problem here shows up there first, and
it is the right thing to include in a bug report.

## "Windows protected your PC" when starting Griddle

Expected. Griddle is unsigned, and SmartScreen flags anything it has not seen widely run. Click
**More info → Run anyway**; Windows remembers.

## Griddle cannot find Steam

Griddle reads Steam's location from the registry. If Steam has never been run on this account, or
was installed by a different user, that entry may be missing.

Set the path yourself in **Settings**, pointing at the folder containing `steam.exe`.

## Artwork applies but Steam does not change

Griddle tells you which of two things happened. If it says the artwork was **written to disk**
rather than applied live, Steam will pick it up when it next starts.

Instant apply needs Steam running *and* the debugging flag in place. Diagnostics reports both. The
flag is created at startup, so if it is missing something removed it — Millennium is known to.

## No results, or fewer than expected

- **Check your filters.** Everything starts ticked, so any unticked box is narrowing the results.
  **Reset filters** rules this out in one click.
- **Check the match.** If the header names a different game, use **Wrong game?**.
- **Some games genuinely have little artwork**, particularly for logos and icons.

If results stop appearing as you scroll, use the **Load more** button below the grid.

## Griddle matched the wrong game

Use **Wrong game?** above the results and search by name. Your choice is remembered per game.

This happens most with remasters and re-releases, which often have a Steam ID that SteamGridDB
does not carry, and always with non-Steam shortcuts.

## Some of my games are missing

The library shows **Installed** games by default. Switch to **All games** for everything Steam
knows about on this PC.

Games no longer in your account — refunded purchases, and withdrawn demos and betas — are left out
deliberately. Steam has no record of them beyond a playtime, so they would appear as blank,
nameless rows.

## My controller does nothing

- Griddle reads the controller **only while its window is focused**.
- Check the pad works elsewhere first — Steam's own controller settings are a good test.
- If Griddle was launched from Big Picture, the Steam Overlay is remapping your controller. That
  is supported, but a custom Steam Input layout can rebind buttons.

## Reporting a bug

Include the version from **Settings → Diagnostics**, and the Steam build number shown beside it.
Issues go to [GitHub](https://github.com/dahui/griddle/issues).
