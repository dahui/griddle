---
title: Uninstalling
description: Removing Griddle, and what is left behind on purpose.
sidebar:
  order: 4
---

## Removing the app

**Installer:** Add/Remove Programs → Griddle → Uninstall.

**Portable:** delete the folder you unzipped.

Neither touches your artwork or your settings. That is on purpose, so reinstalling does not cost
you your API key or your library's appearance.

## Removing what it left behind

Delete these if you want Griddle gone completely.

### Its settings and cache

```
%APPDATA%\Griddle\
%LOCALAPPDATA%\Griddle\
```

The first holds your API key and preferences, the second is a disposable cache.

### The Steam debugging flag

```
C:\Program Files (x86)\Steam\.cef-enable-remote-debugging
```

Delete this to close the local debugging port Steam opens for instant apply.

:::caution
CSS Loader, Decky Loader and Millennium use the same file. If you have any of those installed,
removing it will break them too.
:::

Steam stops listening on that port the next time it starts.

## Your artwork stays

Artwork you applied is Steam's now. It lives in Steam's own custom artwork folder and does not
depend on Griddle being installed, so it survives uninstalling.

To clear it out too, use **Settings → Reset all artwork** *before* uninstalling. Afterwards you
would be deleting files from `Steam\userdata\<your account id>\config\grid\` by hand, and that
folder may hold artwork set by other tools as well.
