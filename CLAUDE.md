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
| `appcache\librarycache\` | **2245** per-appid dirs vs 51 appmanifests — a superset (owned/browsed, not installed). Layout is **sha1-keyed**, see below. **Read-only. Never write here** — Steam re-downloads over it. |
| `userdata\<id>\config\librarycache\<appid>.json` | **Achievement data, not art.** Same name, different thing. Do not confuse with the above. |
| `userdata\<id>\config\licensecache` | Encrypted binary. Dead end for an owned-games list. |

#### 🔴 `appcache\librarycache\` is sha1-keyed — earlier notes here were wrong

Measured across all **2244** cached appid directories on this box
`[VERIFIED-BOX 2026-07-27]`:

| Shape | Appids |
|---|---|
| flat files only — `<appid>/<sha1>.jpg` | **1972** |
| sha1 sub-directories only — `<appid>/<sha1>/<name>.ext` | 137 |
| both | 135 |

Names found *inside* the sha1 sub-directories: `header.jpg` (143), `library_header.jpg` (122),
`library_hero.jpg` (108), `library_hero_blur.jpg` (108), `logo.png` (95),
`library_capsule.jpg` (66), `library_600x900.jpg` (40), `markers.svg` (4). Flat files directly
under `<appid>/`: 4445 `.jpg` + 528 `.png`, all sha1-named.

Both the design research ("modern per-appid subfolder: `header.jpg`, `library_600x900.jpg`, …")
and an earlier line in this file were **wrong** — they omitted the sha1 level entirely. Anything
reading this cache must handle *all three* shapes and must not assume a filename.

This is exactly why the cache is read-only for us: the naming is a Steam implementation detail
that has now changed at least twice. Custom art goes in `userdata/<id>/config/grid/`, which is
stable and documented by usage.

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

**M0 done. M1 spike COMPLETE — every question answered.** Nothing left that can change the
architecture.

| | State |
|---|---|
| **M0** | Cargo + bun workspaces; Tauri shell (PE subsystem = `WINDOWS_GUI`, no console flash); secret scanning (pre-commit + CI); encoding guard. |
| **M1** | **All 11 spike items resolved.** Nothing left that can change the architecture. |
| **M2** | **Offline layer done, including the `shortcuts.vdf` writer** — see the module map below. 112 Rust + 62 TS tests green, clippy clean at `-D warnings`. Verified end-to-end against the real install with `cargo run -p sgdb-core --example scan`. |
| **Next** | `sgdb::client`, `cdp`, `settings`, `steam::apptype`, then **M3** — the first genuinely usable build. |

### `sgdb-core` module map

| Module | What it is |
|---|---|
| `appid` | `AppId` newtype. Signed in `shortcuts.vdf`, unsigned in filenames **and** in the CDP APIs. **Contains no CRC32 function, deliberately** — the folklore algorithm is disproven and the way to never regress is for it not to exist. |
| `vdf::binary` | Binary KV1. Read **and** write, byte-exact, including the extra trailing `0x08`. Validated against the live client in S9. |
| `vdf::text` | Text KV1, read-only. Skips scalar siblings among numbered keys (`contentstatsid`); case-insensitive lookup; handles escapes, comments, `[$WIN32]` conditionals. |
| `logo` | 5-anchor position maths. Mirrors `packages/shared/src/logo.ts`; **both test against one JSON fixture** so they cannot drift. |
| `grid::names` | Filename rules + `AssetType` with Steam's measured ordinals. `siblings()` is the delete-set that keeps exactly one file per asset. |
| `grid::store` | **The only artwork writer.** Sibling cleanup → temp → fsync → rename. Writes a default logo position when a logo has none. Clearing a logo takes its `.json`; clearing the header does not. |
| `steam::locate` | Registry cascade with the lowercase/forward-slash normalisation. `locate_with()` takes the override as a parameter so tests need no `unsafe` env mutation. |
| `steam::account` | `ActiveUser` → `loginusers.vdf` → sole `userdata/` dir. **Refuses to guess** between several accounts. |
| `steam::library` | `libraryfolders.vdf` + `appmanifest_*.acf`. One corrupt manifest never empties the library. |
| `vdf::appinfo` | `appcache/appinfo.vdf` reader. **Not the same format as `vdf::binary`** — v29 keys are u32 string-table indices. Extracts only `common/{type,name,clienticon}`. |
| `steam::apptype` | `common/type` → "does this belong in the library list". Every unknown resolves toward **showing** the app. |
| `sgdb::key` | `ApiKey`. Custom `Debug` prints a fingerprint; **no `Display`, no `Serialize`** — leaking it is a compile error. |
| `sgdb::model` | Response types, every field read off a real response. Only `id` and `url` are required. |
| `sgdb::query` | Endpoint + filter selection. `Dimensions` is a closed set, every value probed. |
| `sgdb::client` | **The only place the key is used.** Concurrency cap 3, backoff with jitter, content-type checked before parsing. |
| `settings` | `%APPDATA%\<AppName>\settings.json`, atomic. **Third and last writer.** A corrupt file is preserved, never overwritten. |
| `settings::dpapi` | `CryptProtectData` round-trip for the API key. Windows-only, with **no plaintext fallback**. |
| `steam::process` | ToolHelp process enumeration; `-shutdown` → poll → relaunch. **The only minter of `SteamStopped`.** Waits on *processes*, never on the registry pid. |
| `steam::shortcuts` | Read/edit/write `shortcuts.vdf`. Round-trip verified on **load**; write needs a `SteamStopped` token *and* re-checks it. Mutation surface is `set_icon` / `clear_icon` only. |

**Verified on this machine 2026-07-27:** Steam found via HKCU, account `16274804` via `ActiveUser`,
1 library, 51 manifests → 51 fully installed → 50 after dropping `228980`, the one shortcut
round-trip verified with its icon resolved and present on disk, and its five artwork files
correctly identified with no ambiguous pairs.

#### The `SteamStopped` token, and why it is not enough on its own

`shortcuts.vdf` writes are gated by a token whose field is private and which has no public
constructor, so only `steam::process` can produce one. Forgetting the check is a **compile
error**.

But a token proves a *past* observation — the user can relaunch Steam a second later. So
`save()` calls `token.reconfirm()` immediately before writing. **The type prevents forgetting to
check; the reconfirm prevents having checked too long ago.** Neither is sufficient alone, and
the distinction is worth keeping when this code is touched.

🔴 **Do not use `ActiveProcess\pid` for this.** It goes to `0` early in shutdown *while
`steam.exe` is still alive* `[VERIFIED-BOX 2026-07-27]` — precisely the window in which Steam
still holds the file and will still rewrite it on the way out. Wait on `steam.exe` **and**
`steamwebhelper.exe`; the helpers outlive the main process. Live check on this box: the gate
refused a real write and named all 8 processes, leaving the file byte-identical.

Two error cases, deliberately distinct because the remedy differs: `StillRunning` ("close Steam")
vs `ShutdownTimedOut` ("we asked; a game may still be closing, or a prompt is waiting").

**The pristine file is preserved once**, at `shortcuts.vdf.sgdb-orig`, and never overwritten
afterwards — later backups would only preserve our own output. A backup failure aborts the save.
After the write the file is **read back and compared**; our own output is also re-parsed and
checked against the document we meant to write before it ever reaches disk.

#### 🟢 `shortcuts.vdf` field shape — measured, not assumed

`[VERIFIED-BOX 2026-07-27]` Full field dump of the real file:

| Field | Type | Note |
|---|---|---|
| `appid` | `0x02` i32 | signed; `0xF1548865` |
| `appname` | `0x01` str | **lowercase key** |
| `exe` | `0x01` str | **lowercase key**; quoted, and contains embedded `"` and `&&` |
| `StartDir` | `0x01` str | **CamelCase key**; quoted; mixed `\` and `/` separators |
| `icon` | `0x01` str | quoted |
| `ShortcutPath`, `LaunchOptions`, `DevkitGameID`, `FlatpakAppID`, `sortas` | `0x01` str | empty here |
| `IsHidden`, `AllowDesktopConfig`, `AllowOverlay`, `OpenVR`, `Devkit`, `DevkitOverrideAppID`, `LastPlayTime` | `0x02` i32 | |
| `tags` | `0x00` map | `"0" → "favorite"` |

Two rules fall out of this, and both are enforced in code:

1. **Key casing is inconsistent** — `appid`/`appname`/`exe` lowercase, `StartDir`/`ShortcutPath`
   CamelCase. Every lookup is case-insensitive, and an existing key keeps the casing it had.
2. **Path values carry literal quote characters *inside* the string.** `exe`, `StartDir` and
   `icon` are all `"C:\..."` here — EmuDeck wrote them that way and Steam accepts both forms.
   A new icon therefore **matches the convention already in the file** (existing `icon` first,
   else `exe`) rather than imposing one.

#### 🟢 `appinfo.vdf` — measured on this box `[VERIFIED-BOX 2026-07-30]`

6,129,997 bytes. Magic `29 44 56 07` = `0x07564429` (**v29**), universe 1, string-table offset
6,052,322. Parsed: **2930 apps, 0 skipped**, and 50/51 installed manifests typed `Game`.

```text
u32  magic  0x07564429  ·  u32 universe  ·  i64 string_table_offset   (v29+)
repeating until appid == 0:
  u32 appid · u32 size · then within `size`:
  u32 info_state · u32 last_updated · u64 pics_token · [20] sha1_text
  u32 change_number · [20] sha1_data (v28+) · binary KV blob
at string_table_offset:  u32 count (9342) · count NUL-terminated strings
```

🔴 **In v29 the KV keys are u32 indices into the string table, not NUL-terminated strings.**
This is why `vdf::binary` cannot be reused. The first app's blob decodes as:

```text
00 | 00 00 00 00                      map, key #0 -> "appinfo"
  02 | 01 00 00 00 | 05 00 00 00      i32, key #1 -> "appid"       = 5
  02 | 02 00 00 00 | 01 00 00 00      i32, key #2 -> "public_only" = 1
08                                    end
```

A parser assuming inline keys would read four bytes of index as the start of a string and
produce confident garbage. Type markers themselves are identical to `vdf::binary`.

🔴 **String-table indices are per-file.** On this build `common`=3, `type`=5, `name`=4,
`clienticon`=363 — properties of *this* file, not the format. The finder predicate is "the
entry whose **resolved** key equals `type`", never "index 5".

Two robustness properties that are worth keeping when this code is touched:

- **Entries are length-prefixed, so a bad blob costs one app, not the file.** The reader
  advances by `size` regardless of what the blob contained. `skipped` is the early-warning
  counter — a blob that does not begin with a map marker counts as skipped rather than
  silently yielding an app with no type, or the signal would never fire.
- **`aligned` checks that the entry list ends exactly at the string-table offset.** If it does
  not, we lost our place stepping through the entries and the app list is quietly incomplete —
  which would reach the user as "some of my games are missing", the hardest kind of bug to
  report. True on this box.

**Failure direction is fixed: unknown means _show it_.** Missing file, unknown magic, app
absent from the cache, or an unrecognised `type` → the app is shown, with the id blocklist as
the floor. `AppType::Other` keeps the unrecognised string rather than collapsing to "not a
game". A missing game is a bug report; a stray tool is a cosmetic annoyance, and the code
should not treat those as equally bad.

#### 🟢 The SteamGridDB API, measured `[VERIFIED-BOX 2026-07-30]`

Reproduce with `$env:SGDB_API_KEY = "<key>"; cargo run -p sgdb-core --example sgdb_probe`
(read-only). The key is read from the environment and **never** from a file in this repo.

| Probe | Result |
|---|---|
| `/games/steam/620`, `/grids`, `/heroes`, `/logos`, `/icons`, `/search/autocomplete` | 200 |
| bad key, **and no key at all** | 401, **empty body** |
| unknown Steam appid | 404, **empty body** |
| a path that is not an endpoint | 404 with a **full HTML page** |
| `?dimensions=1x1` | **400** — invalid filter values are rejected, not ignored |
| `?page=1` | honoured; page 1 ≠ page 0, so infinite scroll works |
| `ETag` on any endpoint | **absent** |

🔴 **Correction to the plan: ETag revalidation is not available.** No endpoint sends an `ETag`,
so the planned "JSON cache by URL+params with ETag revalidation" cannot work. Any cache must be
**time-based**. Combined with the already-recorded absence of `RateLimit-*`/`Retry-After`, every
politeness measure in this client is self-imposed: a concurrency cap of 3, exponential backoff
with jitter, and retries only on 429/5xx/network.

**The `Cache-Control: no-store, no-cache, must-revalidate` is not a considered policy.** It
arrives with `expires: Thu, 19 Nov 1981` and `pragma: no-cache` on *every* endpoint including
static game metadata — that exact trio is PHP's `session_start()` default. Worth knowing before
someone decides we are obliged to honour it and re-fetches on every keystroke.

**Three response envelopes, not one:** `/games/...` returns `data` as a single object;
`/search/autocomplete` returns an array with **no** pagination fields; asset endpoints return an
array with `page`/`total`/`limit`.

🔴 **`icons` rejects `dimensions` outright** — every value 400s, including `8x8`, `16x16`,
`32x32`, `64x64`, `128x128`, `256x256`, `512x512`, `1024x1024`. An earlier draft carried
`512x512` and `1024x1024` as icon dimensions purely because they sounded right; the live probe
caught both, and they are **deleted rather than commented out**. Same principle as the absent
CRC32: a value that cannot be constructed cannot be sent.

🔴 **`grids` serves two of our five slots.** Portrait capsule (`<id>p.png`) and wide header
(`<id>.png`) come from the *same* endpoint, separated only by `dimensions`. Querying `grids`
without dimensions for the Header tab fills it with portrait art that then gets written to the
wide slot — it applies, and looks wrong, which is worse than failing.

**Also verified:** icons legitimately report `width: 0, height: 0`, so never derive an aspect
ratio without checking; `heroes?dimensions=1600x650` is valid but currently matches nothing, so
an empty result there is not a bug.

#### 🔑 How the API key is actually protected

Three layers, and the first is the one that cannot be forgotten:

1. **`ApiKey` implements no `Serialize`.** So it *cannot* be written into `settings.json` by
   accident — the only route in is `Settings::set_api_key`, which DPAPI-wraps it first. The
   encryption is not something a later edit can skip, because the plaintext type will not
   serialise at all. It has no `Display` either, so `format!("{key}")` does not compile, and a
   custom `Debug` prints `ApiKey(e6e2…, 32 chars)`.
2. **DPAPI at rest**, scoped to the current Windows user, with app-specific secondary entropy
   so another process cannot simply call `CryptUnprotectData` on the blob. Off Windows there is
   deliberately **no plaintext fallback** — that would be a build where the secret is silently
   unprotected.
3. **`scripts/check-secrets.sh`** keeps it out of git, by hash rather than by literal.

A test reads the bytes actually on disk and asserts the key is not among them. Another asserts
no `Authorization` header reaches `cdn2.steamgriddb.com` — auth is attached per request, never
as a client default, precisely so image downloads cannot carry it.

**An undecryptable key does not fail the whole load.** A settings file from a different Windows
account still yields every tab preference and filter; only the key is reported as unreadable.
Failing the load would look to the user like every setting had been lost.

#### Seven bugs worth remembering

**`Path::ends_with` matches whole components, not string suffixes.** `p.ends_with("_icon.ico")`
is always false for `4048848997_icon.ico`. Compare `file_name()` instead.

**`StateFlags` is a bitfield, not an enum.** `6` = `StateFullyInstalled | StateUpdateRequired` —
installed *and* update-pending, which is playable; FINAL FANTASY TACTICS reads `6` here. A test
asserting `6` meant "not installed" failed against correct code.

**🔴 The encoding guard only scanned *tracked* files, so new files were unprotected until the
moment they were committed.** A PowerShell `Get-Content -Raw` / `Set-Content` round-trip
mangled every em-dash in a brand-new module, and `check-encoding.py` reported "encoding clean"
— because `git ls-files` does not list untracked files. The window in which corruption is
invisible was exactly the window in which it gets committed. Now uses
`git ls-files --cached --others --exclude-standard`, and the fix was verified by re-creating
the identical byte damage and watching it fail. **A guard that only covers committed files
cannot prevent a bad commit.**

**🔴 `.trim()` before `strip_prefix("Bearer ")` eats the space the prefix needs.** `ApiKey::new`
accepted the input `"Bearer "` as a six-character key literally called `Bearer`: trimming
removed the trailing space, so the prefix no longer matched and the label was kept as the
secret. Fixed by splitting on the first whitespace instead, which is also case-insensitive for
free. The bug was found by a test asserting the *rejection* case — the happy path
(`"Bearer <key>"`) worked perfectly throughout.

**🔴 `f.write_str` in a `Display` impl silently ignores width and alignment.** `AssetType`'s
`{:<13}` did not pad in any table or log line. Use `f.pad()`.

**🔴 A mock cannot lie about `Content-Length`.** A test asserting the download size limit set a
false header; hyper rejects the mismatch, so the test exercised a broken server rather than the
limit. The limit became a config field instead, which is both testable and the right shape — a
policy value, not a constant. It now has a passing control case too, so a limit of zero could
not make it "pass".

**🔴 `\0` followed by a digit is an octal-looking escape, and a test passed anyway.** A binary
fixture written as `b"contentstatsid\0778551\0"` does not mean NUL-then-`778551` — `\077` is read
as one escape, so the intended NUL separator was never there. Clippy's `octal_escapes` caught it;
the test did **not**, because it asserted only the outcome ("one shortcut found") and never
checked that the malformed sibling it was supposed to skip actually existed. It now asserts the
premise — two children, one of them a scalar — before asserting the behaviour. Same lesson as the
focus-tree probes: **a test that cannot fail when its fixture is wrong is not testing anything.**
Write `\x00` in binary fixtures.

### M1 spike — all resolved

Ordered by how much they'd have cost to discover late.

| # | Question | Status |
|---|---|---|
| **S7** | Real `shortcuts.vdf` round-trips byte-exactly | 🟢 **PASS** — 701 bytes, 1 file-level terminator |
| **S1** | Sentinel + restart → is there a `SharedJSContext` on 8080? | 🟢 **PASS** — see below |
| **S2** | 🔴 **Crown jewel.** Capture `__webpack_require__`, find a gamepad `Focusable`, render in BPM and join the focus tree. | 🟢 **PASS** — see "S2 PASSES" below |
| **S6** | 🔴 **CSP probe.** WebSocket to loopback? `cdn2.steamgriddb.com` images? | 🟢 **PASS — best case.** Both allowed. |
| **S5** | Wrap the context-menu factory to splice an item before Properties | 🟢 **PASS** — module `5808`, token `#GameAction_GameProperties` |
| **S2b** | If not: does `keydown`/Gamepad API see controller input in SharedJSContext under BPM? | ⬜ not needed unless S2 render fails |
| **S3** | Live apply over CDP on shortcut `4048848997`. Diff `grid/` before/after. | 🟢 **PASS** — 28 ms, no restart |
| **S4** | Animated WebP labelled `png` — does it animate? | 🟢 **PASS** — animates in desktop **and** BPM |
| **S8** | Does `SetCustomArtworkForApp(..., Icon)` work for a real Steam app? | 🔴 **NO** — silent no-op; see below |
| **S9** | Does a `shortcuts.vdf` write survive `-shutdown` → poll pid→0 → relaunch? | 🟢 **PASS** — and it validated our writer |
| **S10** | Unsigned Tauri exe — Defender? SmartScreen? | 🟡 **Defender clean**; SmartScreen untested |
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

---

### 🟢 S2 PASSES — the Big Picture deliverable is viable

`[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27 — probes 13-18]`

Our `Focusable`, mounted through Steam's own `ModalManager`, joins Steam's gamepad focus
navigation. Proven **positively**, by tree membership:

```
tree[0] 'GamepadUI_Full_Root'                             nodes=216  ours=YES  steam-control=YES
tree[1] 'GamepadUI_Full_Root/PartnerEventOverlayContainer' nodes=1    ours=no   steam-control=no
tree[2] 'GamepadUI_Full_Root/ModalDialogOverlay_Modal_11'  nodes=2    ours=YES
```

Steam allocated a **dedicated child tree for our modal** (`ModalDialogOverlay_Modal_11`),
exactly as it does for its own — and our element also appears in the root tree beside
Steam-rendered controls.

#### The working recipe

```js
// 1. Capture the module registry (no factories executed — read require.m[id].toString()).
let req; window.webpackChunksteamui.push([[marker], {}, r => { req = r; }]);

// 2. React 19.1.1 = module 51745 · Focusable factory = module 28869 export HR
const React = req('51745'), Focusable = req('28869').HR('div');

// 3. Walk the React fiber graph for a prop named /modalmanager/i whose value has ShowModal.
//    CRITICAL: take the one whose BUsePopups() === false — that is the inline manager that
//    renders into the DISPLAYED window. The BUsePopups()===true one renders into
//    SharedJSContext's own document, which is never shown.
// 4. ShowModal takes ONE argument and returns { Close, Update, ClosedPromise }.
const handle = mgr.ShowModal(React.createElement(Body));
```

#### 🔴 Five wrong turns — do not repeat them

1. **A detached `createRoot` never integrates**, in *either* document (probes 6, 8). Rendering
   into BPM's document is necessary but not sufficient; you need Steam's React *tree*.
2. **`28869.sl` is a hook, not a component.** Its destructured props are exactly Focusable's,
   but it returns `{elemProps, navOptions, gamepadEvents}`. Matching prop names is not enough —
   confirm it returns JSX.
3. **`ShowModal` has arity 1.** `showModal(modal, parent?, props?)` is decky-frontend-lib's
   *wrapper* signature, not Steam's method. There is no parent argument; the manager you pick
   *is* the routing decision.
4. **`showModal` as a search term is a red herring** — every literal occurrence in the bundle is
   the native `HTMLDialogElement.showModal()` DOM API.
5. 🔴 **Focus nodes are NOT attached to DOM elements.** Probes 6-16 all reported
   `treeNode: null` using `Object.keys(el)`. That check was meaningless: **Steam's own
   `Focusable` elements show the same nothing** (2 own properties, both React fiber keys, no
   symbols). Membership lives in `g_WindowFocusCoordinator.m_rgTrees[i].tree`, whose root node
   carries `m_Root → m_rgChildren → m_element`.

   **The lesson:** when a probe reports absence, run the identical probe against a
   Steam-rendered control before believing it. Ten probes' worth of "not integrated" was a
   broken measurement, and the control comparison exposed it in one run.

#### Other facts worth keeping

- Focus trees exist **only in Big Picture**: `m_rgTrees` is 0 in desktop mode, 2 with BPM open,
  3 while a modal is up. Any focus test run in desktop mode is inconclusive by construction.
- BPM's document is `SP BPM_uid0` in `g_PopupManager.m_mapPopups`; the desktop window is
  `SP Desktop_uid0`. Steam's popups appear to be React portals rooted in SharedJSContext, which
  is why walking up the fiber tree from a BPM element lands in SharedJSContext's tree.
- Tree entries are `{name, tree, browserContext}`; nodes carry `m_Tree`, `m_Parent`,
  `m_rgChildren`, `m_element`, `m_FocusRing`, `m_nDepth`, `m_FocusableIfEmptyAncestor`.

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

**Steam's own name mapping**, from the same module's `SetCustomArtwork` switch:

```js
case vt.b_A: n = "library_capsule"           // portrait capsule
case vt.KoM: n = "library_hero"
case vt.JoK: n = "store_capsule_main"        // wide capsule
case vt.n4o: n = "library_logo_transparent"
```

The enum members are mangled but those *strings* survive minification, so "the enum whose
members appear beside these asset-name strings" is the durable finder — record that predicate,
never the mangled keys.

---

### 🟢 ELibraryAssetType ordinals — measured, not assumed

`[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]` Each ordinal applied in turn, on **both** a
non-Steam shortcut and a real Steam app, watching which file appeared. Identical results:

| Ordinal | Name | File written |
|---|---|---|
| 0 | Capsule | `<appid>p.png` |
| 1 | Hero | `<appid>_hero.png` |
| 2 | Logo | `<appid>_logo.png` |
| 3 | Header (wide capsule) | `<appid>.png` |
| 4 | Icon | **nothing** |
| 5 | HeroBlur | **nothing** |

decky-frontend-lib's ordering is **correct** — a rare case where the typings held up. Worth
having measured anyway: an off-by-one here would silently write hero art into the capsule slot.

### 🟢 S8 — icons cannot be set through `SetCustomArtworkForApp`

Ordinal 4 writes **no file at all**, for shortcuts *and* Steam apps. The call does not throw;
it takes ~500 ms (vs ~30–50 ms for the working types) and returns normally. A silent no-op.

Consequences for the product:

- **Non-Steam shortcuts** — icons need the file path: write `<appid>_icon.<ext>` into `grid/`
  **and** set the `icon` field in `shortcuts.vdf` (Steam must be shut down; then restart).
  That is what decky-steamgriddb does, and it is why its icon flow prompts for a restart.
- **Real Steam apps** — decky writes `appcache/librarycache/<appid>_icon.jpg`, the **legacy
  flat** layout. This box's cache is sha1-keyed, so that path is `[INFERRED]` dead here.
  Ship the Icon tab **disabled for Steam apps** with an explanation, rather than a control that
  silently does nothing.

⚠️ **One unexplained observation.** The first S8 run passed ordinal 4 against appid 1004640 and
`1004640.png` (the *Header* file) appeared. The systematic sweep afterwards — 6 ordinals × 2 app
types — never reproduced it: 4 wrote nothing and 3 wrote `<appid>.png`. The sweep is 12 data
points against 1 and is treated as authoritative, but the anomaly is recorded rather than
tidied away. If icons ever appear to half-work, start here.

**Test hygiene:** `grid/` was snapshotted before and restored after; every file created for
appid 1004640 was deleted (it had no custom art originally) and the shortcut's five files
verified byte-identical by SHA-256.

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

### 🟢 S4 — animated assets work, and the bytes must come from Rust

`[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27 — maintainer confirmed on screen]`

A 45-frame animated WebP (601,648 bytes, VP8X animation flag set) applied with the mime
argument set to the literal `"png"` lands at `grid/<appid>p.png` and **animates in the desktop
library and in Big Picture**. Extension `.png`, content RIFF/WEBP — Chromium sniffs content, so
Valve's hardcoded `"png"` is not a bug to work around but the mechanism to copy.

SteamGridDB serves animated grids as WebP (full-size) with `.webm` thumbnails; some are APNG
(verified one with `acTL` + 73 `fcTL` frames).

#### 🔴 SharedJSContext cannot read image bytes — CORS

A normal `fetch('https://cdn2.steamgriddb.com/...')` from SharedJSContext fails with
**"Failed to fetch"**. Only `mode:'no-cors'` succeeds, and that response is **opaque**, so the
body cannot be read. Images can be **displayed** there (`<img src>` works — S6) but never
**read**.

So the injected bundle can never fetch-and-encode an asset itself. **Rust downloads and hands
over base64.** This is exactly why decky-steamgriddb ships a Python `download_as_base64`
backend, and it confirms the architecture: `sgdb::client` owns all SGDB HTTP.

The harness models this with `--payload <base64-file>`, which injects
`window.__SGDB_PAYLOAD__` before the probe runs. An 802 KB base64 string crossed CDP fine and
applied in 48 ms.

---

### 🟢 S9 — the shutdown/write/relaunch choreography works

`[VERIFIED-BOX 2026-07-27]` Reproduce with `cargo run -p sgdb-core --example set_shortcut_icon`
(read-only; add `--appid <id> --icon <path> [--shutdown]` to write, `--restore` to put the
pristine backup back). The harness now runs on `steam::shortcuts` + `steam::process` rather than
the throwaway code S9 used, so **what shipped is what was tested**.

Sequence: `steam.exe -shutdown` → poll until both `steam` and `steamwebhelper` are gone →
read/modify/write with **our own `vdf::binary` codec** → relaunch → wait for
`ActiveProcess\pid != 0` and ≥3 helpers → **read back**.

| Check | Result |
|---|---|
| Write applied while Steam was down | 701 → 695 bytes |
| Survived a full Steam startup | ✅ still 695 bytes |
| Icon field after relaunch | ✅ our marker value |
| Steam rejected or rewrote our file | ❌ no — it parsed and kept it |
| Restore (second cycle) | ✅ back to 701 bytes, original SHA-256 |

**This is the strongest test the codec has had.** The round-trip unit tests prove we can
reproduce a file byte-for-byte; this proves the real Steam client accepts a file we *modified*.
The example refuses to write at all if the round-trip check fails first.

⚠️ **Restoring also needs Steam down.** After relaunch Steam holds the modified shortcuts in
memory, so writing the backup while it runs would be overwritten on exit. The restore needed
its own shutdown/relaunch cycle — worth knowing before building any "undo" feature.

🔴 **The existing icon value contains literal quote characters:**
`"C:\Users\...\EmulationStationDE.ico"` — quotes stored *inside* the string, not VDF syntax.
EmuDeck wrote it that way. Steam evidently tolerates both forms (ours was written unquoted and
was accepted), but **preserve whatever is there** rather than normalising, and match the
surrounding convention when writing a new one. Same discipline as the mixed path separators in
`StartDir`.

---

### 🟢 S10 — Defender clean; SmartScreen still unknown

`[VERIFIED-BOX 2026-07-27]` Real-time protection **on**; an on-demand `Start-MpScan` over the
unsigned release build completed with **no detections** and the binary intact.

This is the expected outcome and worth noting *why*: we deliberately avoid the heuristics that
flag Millennium — no DLL proxying, no patched Steam files, no injected `user32.dll`. The CEF
debugger is Valve's own opt-in mechanism.

⚠️ **SmartScreen is NOT covered by this test.** It triggers on Mark-of-the-Web, which a locally
built exe does not carry (`Zone.Identifier` absent, confirmed). A *downloaded* unsigned
installer with no reputation will very likely warn. Re-test with a real download at M8, and
document the click-through. Signing is a v1.1 problem — Trusted Signing wants a 3-year-old
legal entity.

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

#### 🟢 S5 — the context-menu splice point

`[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]` Run `cargo run -p sgdb-core --example
cdp_probe -- --menu`.

**Module `5808`** builds the game context menu (21.6 KB). The Properties item, rendered last:

```js
1 == e.length && !M.Ih.BKioskModeLocked() && (0,n.jsxs)(n.Fragment, { children: [
  (0,n.jsx)(L.K5, {}),                                    // separator
  (0,n.jsx)(L.kt, {                                       // MenuItem
    onSelected: () => this.props.navigator.AppProperties(e[0].appid),
    children: (0,Z.we)("#GameAction_GameProperties") }) ] })
```

| Piece | Meaning |
|---|---|
| `e` | `rgApps` — the selected apps; `e[0].appid` is the target |
| `L.kt` / `L.K5` | MenuItem / separator components |
| `Z.we` | the localization lookup |
| `1 == e.length` | Properties only appears for a **single** selection |
| `BKioskModeLocked()` | and not in kiosk mode — our item should respect both |

The **whole menu in source order** — useful for choosing where "Change Artwork…" belongs:
`ConfirmExitGameTitle`, `UnsavedDataWarning`, `ConfirmStopStreamingTitle`,
`AddToCollectionOption_NewCollection`, `Manage`, `ViewCDKeys`, `ControllerConfiguration`,
`DismissPlayNext`, `BrowseLocalFiles`, **`GameProperties`**, `AllowForChild`, `DenyForChild`,
`FamilyMenu`, `MarkAsPrivate_NoShortcuts`, `RemoveGameLicense`, `DevMenu`,
`DeleteProtonFiles`, `ClearSelectedControllerConfig` (all `#GameAction_*`).

`showContextMenu` (lowercase) exists in exactly one module, **`39590`** — that is the opener.

⚠️ **The anchor is `#GameAction_GameProperties`.** Earlier guesses `#AppProperties_Title` and
`#AppDetails_Properties` scored **zero** — they do not exist on this build. Anchor on the
token, never on `L.kt`/`L.K5`/`Z.we`, which are mangled and will differ next build.

Still not on the critical path — the fallback ladder (global hotkey → desktop-driven) stands
if a future build moves this.

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
