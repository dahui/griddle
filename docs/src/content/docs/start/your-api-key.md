---
title: Your API key
description: Why Griddle asks for your own SteamGridDB key, where to get one, and how it is stored.
sidebar:
  order: 2
---

Griddle asks for a **SteamGridDB API key** the first time it runs. It is free, and it takes about
a minute.

## Getting one

1. Sign in at [steamgriddb.com](https://www.steamgriddb.com/) (a Steam login works).
2. Open your **profile → Preferences → API**.
3. Generate a key and copy it.
4. Paste it into Griddle when it asks, or later under **Settings → API key**.

## Why yours and not one built in

Shipping a shared key inside a downloaded app does not work, for a reason that is easy to observe:
the Decky plugin Griddle replaces has one hardcoded, and it now returns **401** for everyone. A
secret inside a distributed binary gets extracted, over-used, and revoked — and when it is
revoked, every installation stops working at the same moment.

A key you generated is yours, is rate-limited to your own use, and cannot be taken away by
somebody else's behaviour.

## How Griddle stores it

Your key is encrypted with **Windows DPAPI**, scoped to your Windows user account, with
app-specific entropy. In practice:

- It is never written to disk in plain text.
- Another Windows account on the same PC cannot decrypt it.
- It is never sent anywhere except SteamGridDB's own API — in particular it is never attached to
  image downloads from their CDN.

If you copy your settings file to a different machine or a different Windows account, the key will
not decrypt there. Griddle reports that specifically and keeps the rest of your settings, so you
only need to paste the key again.

## Next

[Change your first piece of artwork](/griddle/start/first-artwork/).
