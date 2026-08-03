---
title: Troubleshooting
description: Steam not found, live apply unavailable, missing results, wrong matches, and SmartScreen.
sidebar:
  order: 3
---

**Settings → Diagnostics** reports what Griddle found on your machine: its own version, Steam's
location, your account, and whether instant apply is available. Almost every problem on this page
shows up there first, and it is the right thing to include in a bug report.

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
  value. Pasting `Bearer <key>` is fine, since Griddle strips it.
- **Generate a fresh one** at **profile → Preferences → API** and paste that instead.
- **If it fails in a few seconds with a network message**, the key was never the problem. Griddle
  could not reach SteamGridDB at all. Check your connection and try again.

## Griddle asks for my key again on a new PC

Expected, and nothing has been lost. Your key is encrypted for one Windows account, so a settings
file copied to another machine, or to another account on the same one, cannot be unlocked. Griddle
says so on the welcome screen and keeps every other setting. Paste the key again and you are back
where you were. See [Your API key](/griddle/start/your-api-key/).

## Griddle cannot find Steam

Griddle reads Steam's location from the registry, and that entry is missing if Steam has never run
on this account.

**Start Steam once, then restart Griddle.** That writes the entry, and fixes this nearly every
time.

If Steam is somewhere the registry does not know about, set the `SGDB_STEAM_PATH` environment
variable to the folder containing `steam.exe`. Diagnostics then names it as the source, so you can
see it took effect.

**If Griddle found the wrong Steam** of two installations, Diagnostics names the path and the
registry key it came from, and `SGDB_STEAM_PATH` overrides both.

## Artwork applies but Steam does not change

Griddle tells you which of two things happened. If it says the artwork was **written to disk**
rather than applied live, Steam will pick it up when it next starts.

Instant apply needs Steam running *and* the debugging flag in place. Diagnostics reports both. The
flag is created at startup, so if it has gone missing, something removed it. Millennium is known
to.

## No results, or fewer than expected

- **Check your filters.** Nearly everything starts ticked, so an unticked box is narrowing the
  results. **Reset filters** rules this out in one click.
- **Check the match.** Expand **Filters** and read the button on the right. It names the
  SteamGridDB game these results came from, and it is the only place that name appears.
- **Some games genuinely have little artwork**, particularly for logos and icons.

If results stop appearing as you scroll, use the **Load more** button below the grid.

## Griddle matched the wrong game

Expand **Filters** above the results. The button on the right names the SteamGridDB game Griddle
matched, or reads **Wrong game?** if it matched nothing. Click it and search by name. Your choice
is remembered per game.

Note the heading at the top is your *Steam* game's name and never changes, so a wrong match looks
exactly like a right one until you open Filters.

This happens most with remasters and re-releases, and always with non-Steam shortcuts.

## Some of my games are missing

The library shows **Installed** games by default. Switch to **All games** for your whole library.

**Start Steam if it is closed.** Griddle asks the running client for your library, which is how it
finds games you own but have never launched on this PC. With Steam closed it can only use what
Steam's files remember, and that is a few hundred games short. Griddle offers to start Steam when
it opens; **Settings → Startup** can make it do so without asking.

Games that have left your account are left out on purpose: refunded purchases, and withdrawn demos
and betas. Steam keeps nothing about them but a playtime, so they would show up as blank, nameless
rows.

## My controller does nothing

- Griddle reads the controller **only while its window is focused**.
- Check the pad works elsewhere first. Steam's own controller settings are a good test.
- If you launched Griddle from Big Picture, the Steam Overlay is remapping your controller. That is
  supported, but a custom Steam Input layout can rebind buttons.

## Reporting a bug

Include the **Version** from **Settings → Diagnostics**. Press **Test live apply** on the same
screen and include the Steam build number it reports, which pins the problem to a Steam build as
well as a Griddle one.

Issues go to [GitHub](https://github.com/dahui/griddle/issues).
