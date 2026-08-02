---
title: Troubleshooting
description: Steam not found, live apply unavailable, missing results, wrong matches, and SmartScreen.
sidebar:
  order: 3
---

**Settings → Diagnostics** reports what Griddle found on your machine — its own version, Steam's
location, your account, and whether instant apply is available. Almost every problem here shows up
there first, and it is the right thing to include in a bug report.

## "Windows protected your PC" when starting Griddle

Expected. Griddle is unsigned, and SmartScreen flags anything it has not seen widely run. Click
**More info → Run anyway**; Windows remembers.

## My API key was rejected

Griddle checks a key with SteamGridDB before storing it, so this is caught while you are still
looking at the box rather than at the first game you open. SteamGridDB answers the same way for a
key that is wrong, one that was never valid, and one that has been revoked, so there is nothing
more specific to report.

- **Check you copied all of it.** A key is 32 letters and numbers. If Griddle says what you pasted
  does not look like one, that is usually a half-selected copy, or a label dragged along with the
  value. Pasting `Bearer <key>` is fine — Griddle strips it.
- **Generate a fresh one** at **profile → Preferences → API** and paste that instead.
- **If it fails in a few seconds with a network message**, the key was never the problem — Griddle
  could not reach SteamGridDB at all. Check your connection and try again.

## Griddle asks for my key again on a new PC

Expected, and nothing has been lost. Your key is encrypted for one Windows account, so a settings
file copied to another machine — or another account on the same one — cannot be unlocked. Griddle
says so on the welcome screen and keeps every other setting; paste the key again and everything is
as it was. See [Your API key](/griddle/start/your-api-key/).

## Griddle cannot find Steam

Griddle reads Steam's location from the registry. If Steam has never been run on this account, or
was installed by a different user, that entry may be missing.

Start Steam once and restart Griddle — that writes the registry entry Griddle looks for, and is
the fix in nearly every case.

If Steam is somewhere the registry does not know about, set the `SGDB_STEAM_PATH` environment
variable to the folder containing `steam.exe` and start Griddle from there. Diagnostics then shows
`SGDB_STEAM_PATH` as where it found Steam, so you can tell it took effect.

**Griddle found the wrong Steam.** With more than one installation, the registry decides which one
wins. **Settings → Diagnostics** names the path *and* the registry key it came from, and
`SGDB_STEAM_PATH` overrides both.

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

Include the **Version** from **Settings → Diagnostics**. Press **Test live apply** on the same
screen and include the Steam build number it reports — that pins the problem to a Steam build as
well as a Griddle one.

Issues go to [GitHub](https://github.com/dahui/griddle/issues).
