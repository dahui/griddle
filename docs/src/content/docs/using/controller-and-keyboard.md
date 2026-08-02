---
title: Controller and keyboard
description: The full button map, and how to launch Griddle from Steam Big Picture.
sidebar:
  order: 4
---

Griddle can be driven entirely by a controller or by the keyboard. This is what makes it usable
from the couch, which is the one thing the Decky plugin it replaces had over a desktop app.

## Controller

| Button | Does |
|---|---|
| **Left stick / D-pad** | Move between tiles and controls |
| **A** | Select — apply the artwork, press the button |
| **B** | Back — close a dialog, or return to the previous screen |
| **Y** | Open the menu for whatever is selected (the same as right-clicking it) — an artwork slot's reset menu, or a search result's details |
| **LB / RB** | Move between tabs |

Holding a direction repeats, accelerating as you hold it, so a long library scrolls quickly.

Griddle only reads the controller **while its window is focused**. Alt-tab to a game and your
controller stops driving Griddle.

## Keyboard

| Key | Does |
|---|---|
| **Arrow keys** | Move between controls |
| **Enter** or **Space** | Select |
| **Escape** | Back |
| **Tab** | Move through controls in order; inside a dialog it stays within the dialog |

Arrow keys move the cursor inside a text field rather than the interface, so filtering and typing
a search work normally.

## Launching from Big Picture

Griddle is a normal Windows app, so Steam can launch it like any other:

1. In Steam, **Games → Add a Non-Steam Game to My Library**.
2. Browse to `Griddle.exe` and add it.
3. Launch it from Big Picture like a game.

Your controller works there because Griddle reads it natively rather than through the web browser
its interface is drawn in. That distinction matters: the browser's own controller support breaks
whenever the Steam Overlay is attached, which is exactly the case here.

You also get Steam Input for free — remap Griddle's controls the way you would for any game, and
Griddle sees the result.

## One control a controller cannot reach

The **Sort** dropdown in the library toolbar is a native Windows dropdown, and its popup does not
accept controller input. You can reach and open it with the keyboard. Replacing it is planned.
