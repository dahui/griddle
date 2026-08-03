---
title: The write boundary
description: Why only four modules in Griddle may write files, and how that is enforced.
sidebar:
  order: 3
---

Griddle's worst realistic failure is corrupting a Steam configuration file the user cannot
regenerate. The design treats that as the thing to engineer against.

## Only four modules write

- `grid::store`, for artwork files
- `steam::shortcuts`, for `shortcuts.vdf`
- `settings`, for the settings file
- `cache`, for the disposable cache

Anywhere else, a `fs::write`, `File::create`, `remove_file` or `OpenOptions` **fails CI** unless
the line carries an explicit annotation. Adding a file write outside those modules has to be a
deliberate, visible act rather than something that slips into a diff.

A fifth file, `fsutil`, is on the same allowlist without being a fifth writer. It holds the shared
temp-write-flush-rename dance the other four call, and it knows nothing about what it is writing.
It exists because each of those modules used to carry its own copy, which was three chances to
lose the flush.

`cache` is on the list for a different reason from the other three. It writes only inside a folder
Griddle created and can delete at will, so it is not the irreplaceable-config risk the boundary
exists for. Its guard is a different one: every path derives from its own root and every filename
is a hash, with a test asserting a key like `../../../../windows/system32/evil` cannot escape.

The check scans **every line**, including code after the test module. It used to stop at the first
`#[cfg(test)]`, assuming tests come last, which made a write appended after the test module
invisible.

## Writing artwork safely

Every artwork write does three things, and each one is load-bearing:

1. **Delete same-named files first.** If both a `.jpg` and a `.png` exist for one slot, which one
   Steam uses is undefined and varies by version.
2. **Write to a temporary name, flush to disk, then rename.** The rename is atomic, and it is also
   what makes Steam notice.
3. **Write a logo position** when a logo has none. A logo without one may not render at all.

## Writing `shortcuts.vdf` safely

Steam holds `shortcuts.vdf` in memory and rewrites it on exit, so a write while Steam runs is
discarded. Griddle enforces this with a token that only the process-management code can produce:
forgetting to check is a **compile error**.

A token only proves a past observation, though, and the user can relaunch Steam a second later. So
the write re-confirms immediately beforehand. The type prevents forgetting to check; the re-confirm
prevents having checked too long ago. Neither is sufficient alone.

The file is also backed up once before the first change, re-parsed before being written, and read
back and compared afterwards.

## Never destructively probe

Griddle deletes no file it did not write, with one exception: the same-slot artwork a user's
apply explicitly replaces, which is named in the interface before it happens.

`steam://flushconfig` is banned outright and checked for in CI. It has historically made Steam
forget where its library folders are.
