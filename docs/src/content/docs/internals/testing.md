---
title: Testing
description: What the gate runs, and the testing habits this codebase enforces.
sidebar:
  order: 4
---

## The gate

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\gate.ps1
```

This is what CI runs, in the same order. A green local gate should make a red CI run impossible —
so every check that could differ is delegated to the same script rather than reimplemented. An
inline copy has already drifted once.

It covers: the secret scan, architecture boundaries, file encoding, the third-party notices being
current, `cargo fmt`, Clippy with warnings denied, the Rust tests for both crates, the frontend
tests, and the TypeScript typecheck.

## Habits worth keeping

**A test that cannot fail when its fixture is wrong is not testing anything.** A test asserting
"one shortcut found" passed against a fixture whose malformed sibling did not exist, because the
escape sequence meant to write a NUL byte was parsed as octal. It now asserts the premise before
the behaviour.

**An all-negative suite cannot tell "correctly refused" from "broken fixture".** Three tests
asserting a rejection passed for the wrong reason — malformed JSON, not the identity check they
claimed to test. Only the control case, asserting a valid handshake *succeeds*, caught it.

**Verify a guard by firing it.** Every check here was tested against a real failure before being
trusted: the secret scanner against real leak attempts, the encoding check by recreating the byte
damage, the notices check by making the file stale.

**Pick test vectors that exercise the loop.** A hand-written hash function with a mistyped
constant passed its empty-string vector, because that result is returned before any arithmetic
happens.

## What cannot be automated here

**Keyboard input.** `SendKeys` reaches the window but the embedded browser does not take keyboard
focus from it, so the page never sees the keystroke. The keyboard pass is manual.

**The recursive asset scope.** Steam stores some artwork one directory deeper than the rest, and
granting access non-recursively fails for exactly that minority. It is not unit-testable — launch
the app and confirm a game known to use the nested layout renders.

**Anything involving a real Steam client.** Several behaviours were only ever confirmed against a
live install. Any test that writes into a real Steam library must snapshot the folder first and
restore it afterwards, verifying by hash — it holds artwork a user may have curated by hand.
