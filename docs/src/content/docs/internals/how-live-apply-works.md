---
title: How live apply works
description: Why Griddle changes artwork without a Steam restart, when every other Windows tool cannot.
sidebar:
  order: 2
---

This is the one thing Griddle does that other Windows artwork tools do not, so it is worth
explaining.

## The problem with writing files

Steam Art Manager, SGDBoop and BoilR all write image files into Steam's custom artwork folder.
That works, but Steam reads the folder when it starts, so nothing appears until you restart it.

## What the Decky plugin does instead

The SteamGridDB plugin for Decky Loader, the Steam Deck tool Griddle replaces on Windows, does not
write files at all. It runs inside Steam's own interface, which is a web application, and calls a
function Steam exposes there:

```js
SteamClient.Apps.SetCustomArtworkForApp(appId, imageData, "png", slot)
```

Steam then updates its own artwork, immediately, because it is the one doing it.

## Griddle reaches the same function from outside

Steam's interface runs on Chromium, which supports a standard remote debugging protocol. Steam has
its own opt-in for it, the `.cef-enable-remote-debugging` file. With that in place, a native
Windows application can connect to Steam over a local port and evaluate code in the same context
the Decky plugin runs in.

So Griddle calls Steam's own function. No DLL injection, no patched Steam files, no modified
client.

## Why this does not break on Steam updates

Steam's interface code is minified, and its internal names change with every build. Anything that
depends on finding a particular component in there is fragile, which is why Decky plugins tend to
break after a Steam update.

`SetCustomArtworkForApp` is different. It is not part of the minified bundle at all. Steam's native
host binds it, and Valve cannot rename it without breaking their own client. **The most valuable
feature is the least exposed to a Steam update.**

Griddle deliberately depends on nothing else in there. An earlier design rendered its own interface
inside Steam's Big Picture mode, which meant finding a dozen internal components. It was built,
proven to work, and then removed, because that whole fragile surface existed to serve one feature.

## And when it is not available

Instant apply needs Steam running with the flag in place. When either is missing, Griddle writes
the file to disk instead and says so, and Steam picks it up at the next start.

That fallback is the floor of the design. It needs nothing from Steam at all, which is what makes
Griddle shippable even if Valve moves the API.
