---
title: What Griddle changes
description: Every file and folder Griddle writes to, and everything it deliberately does not touch.
sidebar:
  order: 2
---

Griddle writes inside your Steam installation, so it should say exactly where. This page lists
every path, including the ones you would only find by looking.

## What it writes

### Your custom artwork

```
C:\Program Files (x86)\Steam\userdata\<your account id>\config\grid\
```

Every piece of artwork you apply lands here, named after the game's app ID. This is Steam's own
folder for custom artwork, the same one Steam Art Manager, SGDBoop and Steam itself use.

Applying artwork **deletes the files it replaces**. If a slot already has an image, the old file
goes, including the same image saved under a different extension. Only one file per slot can exist;
leave two behind and Steam's choice between them is anyone's guess.

### Non-Steam shortcut icons

```
C:\Program Files (x86)\Steam\userdata\<your account id>\config\shortcuts.vdf
```

Only when you set an icon for a non-Steam game, and only with Steam closed. While Steam is running
Griddle asks Steam to make the change and lets Steam write the file itself, so it never edits this
behind Steam's back. Before its first direct change it keeps a one-time copy of the original as
`shortcuts.vdf.sgdb-orig`, then reads back what it wrote and aborts if anything does not match.

### Its own settings and cache

```
%APPDATA%\Griddle\settings.json          your API key, filters, preferences
%LOCALAPPDATA%\Griddle\cache\            downloaded thumbnails and search results
```

The cache is disposable. Deleting it costs nothing but a re-download.

### One flag in Steam's folder

```
C:\Program Files (x86)\Steam\.cef-enable-remote-debugging
```

An empty file, created at startup. It is **Valve's own setting**, not a modification to Steam. It
asks Steam to open a local debugging port, which is how Griddle applies artwork without a restart.
CSS Loader and Decky Loader use the identical file.

There is a real cost and it is worth stating plainly. With the flag in place, Steam listens on a
local port that any program already running as you could connect to. Delete the file and that
undoes it completely; Griddle falls back to writing artwork files.

## What it never touches

- **Steam's own artwork** (`appcache\librarycache\`). Read-only, always. Steam re-downloads over
  that folder anyway, so writing there would achieve nothing.
- **Game files, saves, or downloads.** Nothing outside the folders above.
- **`steam://flushconfig`** and similar maintenance commands, which have historically made Steam
  forget where its library folders are.

## What it sends

Griddle talks to two places: **SteamGridDB's API** (searching, with your API key) and **Steam's
public artwork CDN** (fetching the default artwork it shows behind your custom art). Your key is
never attached to CDN requests.

Nothing about your library is uploaded anywhere.
