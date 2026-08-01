---
title: Install
description: Download Griddle, get past the Windows SmartScreen warning, and check you have WebView2.
sidebar:
  order: 1
---

Griddle is a Windows 10/11 app. Download it from the
[latest release](https://github.com/dahui/griddle/releases/latest).

## Portable or installer

Both contain the same application.

| | Use this if |
|---|---|
| **`Griddle-<version>-portable.zip`** | You want to unzip it anywhere and run it. Nothing is added to Add/Remove Programs. |
| **`Griddle_<version>_x64-setup.exe`** | You want a Start menu entry and an uninstaller. |

Your settings and cache live in the same place either way, so you can switch between them without
losing anything.

## Windows will warn you the first time

Griddle is not code-signed, so Windows SmartScreen shows **"Windows protected your PC"** the first
time you run it. This is expected and it is not a virus warning — SmartScreen flags any program it
has not seen many people run before, and signing certificates require a registered company.

To run it anyway:

1. Click **More info**.
2. Click **Run anyway**.

Windows remembers the choice, so it only happens once. If you would rather verify the download
first, every release publishes `SHA256SUMS.txt`; compare it with:

```powershell
Get-FileHash .\Griddle-1.0.0-portable.zip -Algorithm SHA256
```

## WebView2

Griddle draws its interface with Microsoft Edge WebView2, which is **already installed on
essentially every Windows 11 machine and almost every Windows 10 one** — it ships with Windows and
with Edge.

If it is missing, Griddle says so and links to Microsoft's installer rather than failing silently.
The installer bundle handles this for you.

## Next

[Get your SteamGridDB API key](/griddle/start/your-api-key/) — Griddle cannot fetch artwork
without one.
