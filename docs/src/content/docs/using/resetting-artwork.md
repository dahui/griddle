---
title: Resetting artwork
description: Removing custom artwork from one slot or from every game, and exactly what gets deleted.
sidebar:
  order: 5
---

Resetting removes *your* custom artwork so Steam falls back to its own. It never touches Steam's
artwork, which Griddle only ever reads.

## One slot

Right-click a slot in **Current artwork** (or press **Y** on a controller) and choose **Reset**.

That deletes the custom file for that slot, in whichever image format it was saved as. Resetting a
**Logo** also removes its saved position, since the position is meaningless without the logo.

Steam picks its own artwork back up immediately in most cases.

## Every game

**Settings → Reset all artwork** removes every piece of custom artwork Griddle can see for your
Steam account.

:::caution[This includes artwork Griddle did not apply]
Custom artwork is stored in one folder, and nothing in it records which tool wrote it. If you have
previously used Steam Art Manager, SGDBoop, BoilR, or set artwork by hand in Steam, resetting
everything removes that too.

Griddle lists exactly what it is about to delete and asks you to confirm before touching anything.
:::

There is no undo. If you have artwork you curated by hand, copy the folder somewhere first — it is
named on [What Griddle changes](/griddle/notes/what-griddle-changes/).

## What a reset does not do

- It does not remove Steam's own downloaded artwork. Griddle never writes there.
- It does not change your filters, your API key, or any other setting.
- It does not affect games, saves, or anything else in your Steam install.
