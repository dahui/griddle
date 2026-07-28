# SteamGridDB artwork manager for Windows

> **Product name undecided.** The folder `steamdb_loader` is a placeholder — and a doubly wrong
> one: it's SteamGrid**DB** (not SteamDB), and there is no "loader" (the entire point is *not*
> being Decky Loader). Crates/packages use neutral names (`sgdb-core`, `sgdb-app`,
> `@sgdb/shared`) so nothing presumes a brand. Renaming is a mechanical pass before release.

A Windows-native replacement for the **SteamGridDB Decky Loader plugin**. Two deliverables:

- **A — desktop GUI.** Lists the Steam library, browses/applies SteamGridDB artwork. 1:1 with
  the Decky plugin's feature set.
- **B — in-Big-Picture UI.** Gamepad-navigable, injected into Steam's own React tree.

**The insight the whole design rests on:** the Decky plugin doesn't write files. It calls
`SteamClient.Apps.SetCustomArtworkForApp` from inside Steam's JS realm, which is why art applies
*live*. Every existing Windows tool (Steam Art Manager, SGDBoop, BoilR) writes files and needs a
Steam restart. That realm — `SharedJSContext` — is reachable from a native app over Steam's own
CEF remote-debugging port. So we get Decky's behaviour from a normal Windows app, with no DLL
injection and no Millennium.

- **Crates:** `sgdb-core` (all logic) · `sgdb-app` (thin Tauri shell)
- **Packages:** `@sgdb/shared` (logic shared desktop ↔ BPM) · `apps/desktop` · `apps/bpm`
- **License:** GPL-3.0-or-later — load-bearing, not cosmetic. It makes `decky-steamgriddb`,
  `@decky/ui`, and Steam Art Manager (all GPL) legally *adaptable* rather than merely readable.

Full plan: `C:\Users\jeff\.claude\plans\i-want-to-start-valiant-shamir.md`

---

## ⚠️ Read this first: the verification discipline

**Every fact in this document is tagged. Do not add an untagged claim.**

| Tag | Meaning |
|---|---|
| `[VERIFIED-BOX]` | Confirmed read-only on this machine, with the date. The strongest tag. |
| `[VERIFIED-BOX @ CLSTAMP n]` | Read out of Steam's shipped JS bundle. **These expire** — Steam rewrites `steamui/` on update. The stamp says which build it was true for. |
| `[VERIFIED-SOURCE]` | Read in someone's actual source (Valve's bundle, SGDBoop, decky-steamgriddb). Quote it. |
| `[VERIFIED-DOCS]` | The project's own docs. Weaker — docs lie. |
| `[INFERRED]` | Reasoning, analogy, or a third-party blog. **Must be promoted before it becomes load-bearing.** |

This is not bookkeeping. During the design pass, **the single most widely-repeated fact about
non-Steam shortcuts turned out to be false** — see the CRC32 entry below. It is repeated in
practically every tutorial, and four variants of it were computed against the real file before
concluding it simply does not hold on modern Steam. Had it gone in unverified, every non-Steam
game would have had its artwork written to a filename Steam never reads, and the bug would have
looked like "Steam ignores custom art" rather than "we computed the wrong number."

**Additionally: record the *finder predicate*, not just the conclusion**, for anything read out
of `steamui/`. When a Steam update breaks something, the predicate is what you edit.

---

## Verified facts

### Steam layout `[VERIFIED-BOX 2026-07-27]`

| Fact | Detail |
|---|---|
| Steam root | `C:\Program Files (x86)\Steam` |
| `HKCU\Software\Valve\Steam\SteamPath` | `c:/program files (x86)/steam` — **lowercase, forward slashes.** Must normalize. `SteamExe` likewise. |
| `HKLM\SOFTWARE\WOW6432Node\Valve\Steam\InstallPath` | `C:\Program Files (x86)\Steam` — proper backslashes. Read via `RegistryView.Registry32` so process bitness doesn't matter. |
| `ActiveProcess\ActiveUser` | `0xf85574` = `16274804` = the `userdata\` folder name. **`0` when Steam is down** → fall back to `loginusers.vdf` highest `Timestamp`. |
| Account arithmetic | `76561197976540532 − 76561197960265728 = 16274804` ✓ |
| Steam dir writable | **Without elevation.** The `.cef-enable-remote-debugging` sentinel needs no admin. |
| `libraryfolders.vdf` | Exists in **both** `config\` and `steamapps\`, byte-identical. Prefer `config\`. Modern nested format. |
| `appmanifest_*.acf` | 51 installed. `StateFlags & 4` = fully installed. |
| `appcache\librarycache\` | **2245** per-appid dirs vs 51 appmanifests — a superset (owned/browsed, not installed). Modern per-appid layout: `header.jpg`, `library_600x900.jpg`, `library_hero.jpg`, `library_hero_blur.jpg`, `logo.png`, `<sha1>.jpg`. **Read-only. Never write here** — Steam re-downloads over it. |
| `userdata\<id>\config\librarycache\<appid>.json` | **Achievement data, not art.** Same name, different thing. Do not confuse with the above. |
| `userdata\<id>\config\licensecache` | Encrypted binary. Dead end for an owned-games list. |

**Parse `libraryfolders.vdf` defensively:** some client versions emit *scalar* siblings (e.g.
`contentstatsid`) among the numbered object keys. Skip any child whose value is not a map. This
is the single most common breakage in third-party parsers. `[VERIFIED-SOURCE — steamlocate-rs #3]`

### Non-Steam shortcuts

| Fact | Tag |
|---|---|
| `shortcuts.vdf` is 701 bytes, one entry (EmulationStationDE) | `[VERIFIED-BOX 2026-07-27]` |
| Ends with **four** consecutive `0x08` — one more than the nesting depth (`tags` / shortcut `"0"` / root). The extra is a file-level terminator. A naive writer emits three. | `[VERIFIED-BOX 2026-07-27]` |
| `appid` field = `65 88 54 f1` LE = `0xF1548865` = `-246118299` signed. Grid files on disk are named `4048848997*` = the **unsigned** form. Signed in file, unsigned in filename. | `[VERIFIED-BOX 2026-07-27]` |
| `StartDir` mixes separators: `C:\Users\jeff\AppData\Roaming/EmuDeck/EmulationStation-DE`. **Never normalize round-tripped fields.** | `[VERIFIED-BOX 2026-07-27]` |
| **Steam holds `shortcuts.vdf` in memory and rewrites it on exit — a write while Steam runs is silently discarded.** Art files in `grid/` are exempt. | `[VERIFIED-SOURCE]` |

#### 🔴 The CRC32 appid algorithm is FOLKLORE, and it is wrong

Every tutorial says a non-Steam shortcut's appid is
`crc32_ieee(exe + appname) | 0x80000000`. **Tested against the real shortcut on this box:**

| Input variant | Computed | Actual |
|---|---|---|
| `exe + appname` | `0xBC6181EC` | `0xF1548865` |
| `appname + exe` | `0xCF3552B5` | `0xF1548865` |
| `exe` unquoted `+ appname` | `0xEF64DEAE` | `0xF1548865` |
| `exe` outer-quotes-stripped `+ appname` | `0xDDBC17E8` | `0xF1548865` |

Modern Steam assigns a **random** appid in the high-bit-set range.
`[VERIFIED-BOX 2026-07-27]`, corroborated by [ValveSoftware/steam-for-linux#9463] (still open).

**Rule: always READ `appid` from `shortcuts.vdf`. Never compute it.** Enforced structurally —
`sgdb-core` contains no CRC32 function at all, so there is nothing to regress to. Only use a
generated id when *creating* a brand-new shortcut, where Steam honours whatever we wrote.

### Steam's JS surface `[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]`

| Fact | Finder / evidence |
|---|---|
| Build stamp is readable from **both** disk and the live page | `steamui\library.js` line 1 is `var CLSTAMP="10840511";` and `steamui\changelist.txt` contains exactly `10840511` |
| Module-discovery hook exists | `window.webpackChunksteamui` present in `library.js` |
| The apply API exists **and Valve hardcodes the mime** | `steamui\chunk~2dcc5aaf7.js` contains `SetCustomArtworkForApp(e,r,"png",t)` — Valve's own code passes literal `"png"` regardless of the actual bytes. This is why animated WebP written as `<appid>p.png` animates: Chromium sniffs content, not extension. |
| Logo position payload shape | same chunk: `SetCustomLogoPositionForApp(e.appid,JSON.stringify({nVersion:1,logoPosition:t}))` |
| 🔴 **Name-based module lookup is impossible** | Asset-type enum members appear only as mangled exports (`c.VYj`, `c.JoK`, `c.KoM`, `c.n4o`, `c.b_A`). Every finder must be **structural** — shape, value, or localization-token anchored. |

### Baseline environment `[VERIFIED-BOX 2026-07-27]`

- Port 8080: **no listener**. `.cef-enable-remote-debugging`: **absent**. No proxy `user32.dll`
  → Millennium genuinely not installed. Clean slate.
- Toolchain: Rust 1.97.0 (MSVC only), Python 3.11, git 2.54, **bun 1.3.14** (installed by this
  project — was absent). No Node/npm.
- Steam running as pid 15844 with 7 `steamwebhelper` children.

---

## The rules

### No silent failure

Workspace lints make these guarantees, not preferences: `unused_must_use`,
`let_underscore_must_use`, `unwrap_used`, `expect_used` are all **deny**. An ignored `Result`
must not compile, and `let _ = ...` is a build failure. `-D warnings` in CI.

`thiserror` in core — the UI must distinguish "Steam is running, can't write shortcuts" from
"network timeout". `anyhow` only in `sgdb-app`.

### The write boundary (CI-enforced)

**Only `grid::store`, `steam::shortcuts`, and `settings` may write files.** A grep for
`fs::write|File::create|remove_file` outside those three fails CI.

This project's failure mode is corrupting a user's irreplaceable Steam config. Keep the write
surface small enough to audit by grep.

### 🔑 Secrets never enter git

The SteamGridDB API key is a **per-user secret**. Every API v2 endpoint 401s without one
(verified), so a key *will* get pasted into a terminal, a test, or a config during
development. This is enforced, not trusted:

| Layer | What it does |
|---|---|
| `scripts/check-secrets.sh` | The single implementation. `--all` for CI, no arg for staged. |
| `.githooks/pre-commit` | Thin wrapper. Enable once: `git config core.hooksPath .githooks` |
| CI job `secrets` | Runs the same script **plus scans full history** — a fresh clone has no hooksPath, so CI must not depend on the hook. |
| `.gitignore` | `.env*`, `*.key`, `*.secret`, `secrets.json`, `**/apikey.txt` |

It catches four things: a 32-hex literal assigned to an API-key-shaped name; a literal
`Authorization: Bearer <32hex>`; decky-steamgriddb's hardcoded key by value; and **the
maintainer's own key by SHA-256**, which catches a bare paste with no surrounding context.
Storing the *hash* lets the script name a specific key without containing it.

All four paths were tested against real leak attempts, and the false-positive guard (a public
`cdn2.steamgriddb.com` asset hash in a URL) was tested too. A guard that has never been fired
is not known to work.

**Dev-time key handling:** `SGDB_API_KEY` env var or a gitignored `.env`. At runtime it is
DPAPI-wrapped under `%APPDATA%`, owned solely by `sgdb::client` — it must never reach the
frontend or the injected bundle (that JS realm also runs Valve's code and CSS Loader's).

> **Do not ship our own key.** decky-steamgriddb's hardcoded key — a 32-hex string that is
> the hex encoding of an ASCII phrase naming the loader, findable in `src/constants.ts` of
> that repo — now returns **401**. `[VERIFIED-BOX 2026-07-27]` A shared secret inside a
> distributed binary gets scraped, abused and revoked, and then every install breaks at once.
> That is the concrete argument for asking each user for their own key, and it is a better
> one than "the ToS says so".
>
> The literal is deliberately *not* reproduced here: `scripts/check-secrets.sh` blocks it by
> hash, so writing it into this file would make the documentation fail its own check. (It
> did, on the first attempt.)

### Never destructively probe

**Delete no file we did not write** — except the same-base-name art siblings inside `grid/`
that a user-initiated apply explicitly replaces, and that deletion is named in the UI before it
happens.

`steam://flushconfig` is **banned** (it has historically made Steam forget its library folder
locations). In the CI grep alongside the write-boundary check.

### Grid writes

Learned from SGDBoop `[VERIFIED-SOURCE]`, all three steps load-bearing:

1. **Clean siblings first.** Delete existing `.jpg`/`.jpeg`/`.png` of the same base name. If
   two extensions coexist, which one Steam picks is undefined and version-dependent.
2. **Write `<final>_temp`, fsync, then `rename`.** The rename is what makes the client notice.
3. **Writing a logo always writes `<appid>.json` too** if no position exists
   (`{"nVersion":1,"logoPosition":{"pinnedPosition":"BottomLeft","nWidthPct":50,"nHeightPct":50}}`).
   A custom `_logo` with no `logoPosition` may not render at all.

Filenames: `<id>p.png` portrait · `<id>.png` wide/header (no suffix) · `<id>_hero.png` ·
`<id>_logo.png` · `<id>_icon.<ext>` · `<id>.json` logo position.

---

## Where we are

**M0 in progress.** Workspace scaffolded, bun installed, Cargo workspace green.

| | State |
|---|---|
| **Done** | Cargo workspace (`sgdb-core` + `sgdb-app`), workspace lints, `vdf::binary` codec. **🟢 S7 PASSED** — the real 701-byte `shortcuts.vdf` round-trips byte-for-byte, 12 unit tests + 2 gated integration tests green. |
| **Next** | Tauri shell (M0), then the **M1 spike** — S2 first, it's the only item that can change the shape of the project. |

### The M1 spike — answer these before building on them

Ordered by how much they'd cost to discover late. Each has a fallback, so none can silently
sink the project.

| # | Question | Status |
|---|---|---|
| **S7** | Real `shortcuts.vdf` round-trips byte-exactly | 🟢 **PASS** — 701 bytes, 1 file-level terminator |
| **S1** | Sentinel + restart → is there a `SharedJSContext` on 8080? | 🟢 **PASS** — see below |
| **S2** | 🔴 **Crown jewel.** Capture `__webpack_require__`, find a gamepad `Focusable` + `showModal`, render a box in BPM and **move controller focus onto it.** | 🟡 **mechanism PASS**, render+focus still unproven |
| **S6** | 🔴 **CSP probe.** WebSocket to loopback? `cdn2.steamgriddb.com` images? | 🟢 **PASS — best case.** Both allowed. |
| **S5** | Wrap the context-menu factory to splice an item before Properties | 🟡 lead found, anchor not yet identified |
| **S2b** | If not: does `keydown`/Gamepad API see controller input in SharedJSContext under BPM? | ⬜ not needed unless S2 render fails |
| **S3** | Live apply over CDP on shortcut `4048848997`. Diff `grid/` before/after. | 🟢 **PASS** — 28 ms, no restart |
| **S4** | Animated WebP labelled `png` — animates in desktop library? in BPM? in WebView2? **Three separate answers.** | ⬜ |
| **S8** | For a **real Steam app**, does `SetCustomArtworkForApp(..., Icon)` do anything on the modern `librarycache/<appid>/<sha1>.jpg` layout? Decky targets the *legacy flat* layout, `[INFERRED]` dead here. | ⬜ |
| **S9** | Does a `shortcuts.vdf` write survive `-shutdown` → poll pid→0 → relaunch? **Read back after relaunch**, not before. | ⬜ |
| **S10** | Unsigned Tauri exe — Defender? SmartScreen? | ⬜ |
| **S11** | SGDB API through Cloudflare: 200 or 403? | 🟢 **PASS** — 200 either way; see below |

### Spike results `[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]`

Reproduce with `cargo run -p sgdb-core --example cdp_probe` (add `--probe2` for the
follow-up, `--status` for a no-connection report). Both probes are read-only.

**S1 — the realm is reachable and is unmistakably Steam.**
CEF reports `Chrome/126.0.6478.183`, CDP `1.3`, UA `…Valve Steam Client Safari/537.36`.
15 targets; `SharedJSContext` is `https://steamloopback.host/index.html?…`. In-realm:
`CLSTAMP` = `10840511` **matching `changelist.txt` on disk exactly** — the module-map cache
key works. `SteamClient` present with **48** keys; `appStore`, `appDetailsStore`,
`collectionStore`, `SteamUIStore` all present.

**Apply API is present**, all four functions: `SetCustomArtworkForApp`,
`ClearCustomArtworkForApp`, `SetCustomLogoPositionForApp`, `ClearCustomLogoPositionForApp`,
plus `ReportLibraryAssetCacheMiss`. All report `.length === 0` (native/bound — arity is not a
usable signal, so feature-detect with `typeof`, never by parameter count).

**S2 — module discovery works, without executing anything.**
`webpackChunksteamui.push([[marker], {}, r => …])` captures `__webpack_require__`.
**2564 modules, 0 unreadable** via `require.m[id].toString()` — source is readable *without*
running a single factory, unlike Decky's and Millennium's execute-everything approach.

Source-text anchor counts on this build:

| Anchor | Modules | Anchor | Modules |
|---|---|---|---|
| `GamepadUI` | 198 | `SetCustomArtworkForApp` | 3 |
| `Focusable` | 16 | `ModalRoot` | 3 |
| `showModal` | 3 | `SetCustomLogoPositionForApp` | 1 |
| `SliderField` | 1 | | |

**Focus system located: module `4690`, export `Bp`.** Its source is
`class R{m_Tree;m_Parent;m_rgChildren=[];m_ActiveChild;m_iLastActiveChildIndex=-1;…m_FocusRing;…m_FocusableIfEmptyAncestor…}`
— Steam's real gamepad focus-navigation tree node, not a coincidental name match.

#### The React layer — probes 3-6

| Piece | Location |
|---|---|
| **React 19.1.1** | module `51745`, module-level export |
| **ReactDOM** (`createRoot`; no `render`) | module `98131` |
| **Focusable component factory** | module `28869`, export **`HR`** |
| Focus React contexts | module `28869`, exports `Mg`, `TJ`, `sQ` (all `Symbol(react.context)`) |
| Props-splitting hook (**not** a component) | module `28869`, export `sl` |
| Low-level focusable div renderer | module `28869`, export `D0` (needs a `node` prop) |

**`Focusable` is built by a factory, not exported directly:**
```js
HR = function L(e, t) { const r = S(e); return c.forwardRef((n, i) => I(e, r, n, i, t)); }
// so:  const Focusable = require('28869').HR('div')
```
Verified: that returns a `Symbol(react.forward_ref)` component which renders
`<div class="Focusable">…</div>` — Steam's own class name.

Relevant **globals** (these matter because focus registration is global, not purely React
context): `FocusNavController` (`m_rgAllContexts`, `m_ActiveContext`, `m_rgGamepadInputSources`,
`m_navigationSource`), `g_WindowFocusCoordinator` (`m_rgTrees`, `m_mapChildTreeCleanup`),
`g_PopupManager` (`m_mapPopups`), `SteamUIStore` (`m_GamepadNavigationManager`, `m_WindowStore`).

#### 🔴 Two mistakes to not repeat

**1. `28869.sl` is a hook, not a component.** It was identified as `Focusable` because its
destructured props are exactly Focusable's (`autoFocus`, `preferredFocus`, `noFocusRing`,
`navKey`, `fnCanTakeFocus`, …). But it *returns an object*
(`{elemProps, navOptions, gamepadEvents}`), so React rendered nothing and the failure looked
like "injection doesn't work". **Matching on destructured prop names is not sufficient to
identify a component — check that it returns JSX.**

**2. A detached React root is not focus-integrated.** Mounting `HR('div')` into our own
`createRoot` renders correctly but is inert:

| Signal | Result |
|---|---|
| Renders | ✅ `<div class="Focusable">probe</div>` |
| `tabIndex` | `-1` |
| `.focus()` | did not stick |
| Steam focus-tree node on the element | none |
| `g_WindowFocusCoordinator.m_rgTrees` delta | 0 |
| `FocusNavController.m_rgAllContexts` delta | 0 |

A plain `<div>` control rendered fine in the same detached root, so `createRoot` is not the
problem — the component needs one of the module-`28869` context providers above it.

**Therefore: mount inside Steam's own tree.** This independently confirms the plan's choice of
`showModal` over patching Steam's router — Steam's modal system mounts into their tree with
every provider already in place.

#### Resolved in Big Picture — probe 7 `[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]`

Re-run with **BPM open and a controller connected**. The focus subsystem is unambiguously
live this time:

| Signal | Desktop mode | BPM + controller |
|---|---|---|
| `g_WindowFocusCoordinator.m_rgTrees` | 0 | **2** |
| `m_rgGamepadInputSources` | — | **10** |
| `m_navigationSourceSupportsFocus` | — | **true** |
| `m_navigationSource` | — | `{eActivationSourceType:1, nActiveGamepadIndex:0}` (controller seen) |
| Detached-root mount | not integrated | **still not integrated** |

So the dormancy theory was **wrong** — the negative result was not an artifact. Even with
gamepad navigation fully active, our detached root produced no tree node, no `m_rgTrees` delta
(2 → 2), and `focus()` did not stick.

#### 🔴 The root cause: BPM has its own document

`SharedJSContext` is the shared **JS realm**, but it is *not* the document BPM renders into.
With BPM open, inside SharedJSContext: `document.body.className` is **empty** and
`querySelector('[class*="gamepad" i]')` finds **nothing**, while the CDP target list gains a
separate page titled **`Steam Big Picture Mode`**. SharedJSContext's own URL also changed to
`https://steamloopback.host/routes/library/home`.

Appending to `document.body` in SharedJSContext therefore puts the element in a document BPM
never displays. That, not context providers, is why nothing integrated.

**The BPM document is reachable** — probe 8 confirmed `g_PopupManager.m_mapPopups` exposes it:

| Popup key | `document.title` | gamepad DOM |
|---|---|---|
| **`SP BPM_uid0`** | `Steam Big Picture Mode` | **yes** |
| `contextmenu_13_uid0` | `Menu` | no |

Creating an element with `bpmDoc.createElement` (a node from a foreign document cannot be
appended) and mounting a `createRoot` there **renders correctly** — `<div class="Focusable">`
appears inside BPM's own document.

**But it is still not focus-integrated**: `tabIndex -1`, no tree node, `m_rgTrees` 2 → 2,
`focus()` did not stick, `onFocusWithin` never fired.

#### 🔴 Conclusion: the right document is necessary but not sufficient

`Focusable` needs to be inside Steam's React **tree** — specifically under the context
providers from module `28869` (`Mg`, `TJ`, `sQ`). A fresh `createRoot` has no provider chain
no matter which document it renders into.

Three ways forward, in preference order:

1. **Steam's `ModalManager`** — mounts within Steam's tree with every provider in place. Found;
   see below.
2. **Patch an existing Steam component** to render our children (Decky's approach). More
   surface area to break on a Steam update.
3. **Supply the contexts ourselves** — wrap our subtree in `Mg`/`TJ`/`sQ` providers with values
   read from a live Steam-rendered subtree. Most fragile; last resort.

`[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27 — probes 6, 7, 8]`

#### ✅ ModalManager located — module `3673`, export `SZ` (probe 10)

A class whose prototype is exactly the API we need:

```
ShowModal · ShowModalInternal · ShowPortalModal · RemoveModal
modals · active_modal · SetUsePopups / BUsePopups · SetOnlyPopups
SetBrowserInfo / GetBrowserInfo · SetCenterPopupsOnWindow
RegisterOnModalShownCallback · RegisterOnModalHiddenCallback
RegisterMeasureModalCallback · RequestModalMeasure · RegisterOverlay
```

Module `36437` export `L` is the modal **host** component, taking
`{ModalManager, bRegisterModalManager, DialogWrapper, bUseDialogElement, rctActiveContextMenus}`
— it renders a manager, it is not the manager. `ShowPortalModal` is the interesting one for
BPM: a portal is how you render into a different window's tree while staying inside the React
tree that owns the focus contexts. `[INFERRED — VERIFY]`

#### ✅ Steam's own custom-artwork flow — module `87498`

This module contains both `CloseModal` and `SetCustomArtworkForApp`: it is Steam's *own*
"set custom artwork" implementation, so it defines the convention to match rather than invent.

```js
let a = await fetch(s).then(e => e.blob()), o = new FileReader;
o.onload = () => {
  let r = o.result.toString();
  r = r.slice(r.indexOf("base64,") + 7);          // strip the data-URL prefix
  SteamClient.Apps.SetCustomArtworkForApp(e, r, "png", t)   // <- literal "png", always
};
o.readAsDataURL(a)
```

Confirms three things at once: the base64 payload is **bare** (no `data:` prefix), the mime
argument is **hardcoded `"png"` regardless of actual bytes**, and this is Valve's own code
doing it — not just a Decky convention we were copying on faith.

**Asset-type enum members can be de-mangled by their asset filenames.** The same call site
maps mangled members to Steam's own art names:

| Mangled | Asset name |
|---|---|
| `vt.JoK` | `store_capsule_main` |
| `vt.n4o` | `library_logo_transparent` |
| `vt.b_A`, `vt.KoM` | (also used; names not yet captured) |

Since the *strings* survive minification but the member names do not, the durable finder is
"the enum whose members are used alongside these asset-name strings" — record that predicate,
not the mangled keys. `[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]`

---

### 🟢 S3 — live apply works. This is the whole thesis of the project.

`[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27 — probe 11, maintainer confirmed visually]`

Applied a 149-byte magenta PNG to shortcut `4048848997` (EmulationStationDE) over CDP:

```js
await SteamClient.Apps.SetCustomArtworkForApp(4048848997, bareBase64, 'png', 0 /* Capsule */)
```

| Observation | Result |
|---|---|
| Call duration | **28 ms**, returns `undefined` |
| `grid/4048848997p.png` | 10068 B → **149 B** (hash changed) |
| Every other file in `grid/` | untouched |
| **Library capsule updated with NO Steam restart** | ✅ **confirmed on screen by the maintainer** |

**This is the entire justification for the project.** Steam Art Manager, SGDBoop, BoilR and
every other Windows tool require a Steam restart to show new art. We do not.

Three further facts from the same run:

1. **The unsigned appid is the key for the JS API too**, not just for filenames.
   `appStore.GetAppOverviewByAppID(4048848997)` returned `EmulationStationDE` with
   `BIsShortcut() === true`. So `UnsignedAppId` is the type that crosses the CDP boundary.
2. **Steam overwrites in place and does its own sibling cleanup** — it replaced the existing
   `.png` rather than adding a `.jpg` alongside. Our *file-fallback* path still needs
   `cleanup_siblings()`, because it writes to disk directly with no client involved.
3. `appDetailsStore.GetCustomVerticalCapsuleURL` does **not** exist under that name — it
   returned null. Another name-based guess that missed; the UI-refresh signal has to be found
   by call site if we ever need it. (We do not: the client refreshed itself.)

**Test hygiene:** `grid/` was backed up to `%TEMP%\sgdb_grid_backup` before the write and
restored after, with all five files verified byte-identical by SHA-256. Any future test that
writes to a real library must do the same — this directory holds artwork a user may have
curated by hand and cannot regenerate.

---

### 🟢 S11 — SGDB API works, and the Cloudflare fear was overstated

`[VERIFIED-BOX 2026-07-27]` Same request, three ways, with a real API key:

| Request | Result |
|---|---|
| `/search/autocomplete/portal`, **default (bot-ish) UA** | **HTTP 200** |
| `/search/autocomplete/portal`, descriptive UA | HTTP 200 |
| `/grids/steam/620?dimensions=600x900&limit=3` | HTTP 200, real data |

**Correction to the plan.** The design research said a bare HTTP client gets a Cloudflare 403
and that a browser-like UA is required. **That is false for API v2 with a valid Bearer token** —
a default client UA returned 200. The 403s seen during research were on browser-gated *pages*
(`/api/v2` docs, `/faq`), not the API. Risk item "Cloudflare 403 on a bare HTTP client" is
therefore lower than recorded; still send a descriptive UA as etiquette and for
identifiability, but it is **not** load-bearing. `[INFERRED → corrected to VERIFIED-BOX]`

**No rate-limit headers exist.** Full response header set is
`Connection, Content-Length, Content-Type, Date, Set-Cookie, Server, expires, Cache-Control,
pragma, Nel, cf-cache-status, Server-Timing, Report-To, CF-RAY, alt-svc` — no `RateLimit-*`,
no `Retry-After`. So there is nothing to honour reactively: the concurrency cap (3), backoff
on 429, and ETag caching have to be self-imposed, exactly as planned.

Response shape confirmed: `data[]` with `id`, `width`, `height`, `mime`, `style`, `nsfw`,
`humor`, `epilepsy`, `author.name`.

🔑 **The API key is a user secret.** It is **not** in this repo and must never be committed —
runtime storage is DPAPI-wrapped under `%APPDATA%`, per `settings`. Note this is also the
reason we cannot be 1:1 with the Decky plugin, whose key is hardcoded and explicitly
non-reusable.

#### `showModal` — the name is a red herring

Every literal `showModal` in the bundle is the **native `HTMLDialogElement.showModal()` DOM
API**, not a Steam helper. Module `36437` is the real modal component:
```js
i.useLayoutEffect(() => { o && s.current.showModal(); }, [o]),
  (0,n.jsx)("dialog", { ref: s, className: S.ModalDialog, onClose: … })
// immediately followed by:  function k(e){const{ModalManag…
```
So Steam's modals are `<dialog>` elements driven by a `ModalManager` in module `36437`. Three
separate name-based searches produced false positives before this (a video-theater component,
`SettingsModalRoot` which is a CSS class *string*, and the DOM API itself) — **stop searching
by name; search by call site.**

**S6 — the best available outcome; the feared case did not happen.**

| Check | Result |
|---|---|
| `new WebSocket('ws://127.0.0.1:1')` | **allowed by CSP** → loopback WS is the RPC transport |
| `<img src="https://cdn2.steamgriddb.com/thumb/….jpg">` | **loaded, 419×196** |
| `<img src="https://shared.steamstatic.com/…/440/header.jpg">` (control) | loaded, 460×215 |
| `fetch('https://cdn2.steamgriddb.com/…', {mode:'no-cors'})` | `ok type=opaque status=0` |
| CSP `<meta>` policies in the realm | none |

So BPM can load SteamGridDB thumbnails **directly from the CDN**, and never needs art
proxied as base64 over CDP — the one outcome the plan flagged as genuinely painful is off the
table.

⚠️ **Distinguish a 404 from a CSP block.** The first probe used
`cdn2.steamgriddb.com/favicon.ico` and got `error`, which read as "CSP blocked us". It was a
404. `<img>` `onerror` cannot tell the two apart — always pair it with a `fetch()` (whose
rejection message names CSP) and a control URL that is known to load.

**S5 — not yet answered.** Guessed anchors `#AppProperties_Title` and `#AppDetails_Properties`
both scored **zero**; they do not exist on this build. Real lead: **`#AppDetails_ManageDLC` in
module `3651`**, which is app-detail menu territory. The token scan is in `probe2.js` — widen
its `interesting` regex and re-run. Not on the critical path: the entry-point fallback ladder
(global hotkey → desktop-driven) does not depend on it.

---

## The reliability idea worth protecting

Steam's export names are mangled per build, so module discovery is structural and inherently
fragile. That's why Decky plugins feel unreliable after a Steam update — they break silently.

But `CLSTAMP` is readable from *both* `changelist.txt` on disk and the live page. So: cache the
resolved module map keyed by build stamp; on a stamp change, re-run every finder and **diff
against the cached map**. A silent break becomes:

> *"Steam updated to build 10850000. 9 of 11 components re-found; `AppContextMenu` and
> `SliderField` not found — the context-menu entry is unavailable, use the F8 hotkey."*

~100 lines. Nothing in Decky or Millennium does this, and it is the main reason to build this
rather than keep fighting the plugin. Each finder is independently nullable and each feature
declares which it needs, so losing `SliderField` costs the zoom slider, not the app.

---

## Deliberate divergences from the Decky plugin

| Divergence | Why |
|---|---|
| **User supplies their own SGDB API key** | Decky's is hardcoded with an explicit *"attempting to use this in your own projects will cause you to be automatically banned and blacklisted"*. Non-negotiable. `[VERIFIED-SOURCE]` |
| **BPM UI is a modal, not a route** | Decky registers a route because it *has* `routerHook`. We'd have to patch Steam's minified router. `showModal` is a smaller, more stable target with the same UX. |
| **Installed games + non-Steam shortcuts only** | Fully offline; no Steam Web API. `librarycache`'s 2245 dirs are a known future path to an owned-games view — noted, not built. |
| **No MOTD, donation modal, or tutorial video** | Decky-store furniture. The first-run API-key flow replaces the tutorial. |
| **Library style tweaks ship behind "Experimental"** | Square Capsules / Matching Recents / Capsule Glow patch Steam's own library rendering *globally* — the most fragile surface in the product. Same features, honest labelling, individually disableable. |
| **Plus, not in Decky: a diagnostics screen and the build-stamped module map** | The reliability gap is the actual reason to build this. |

Matching Decky's restraint, explicitly **not** added: favorites, download history,
upload-to-SGDB, bulk apply, HeroBlur editing. Bulk apply in particular is the fastest route to
an SGDB rate-limit problem.
