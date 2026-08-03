# Griddle — SteamGridDB artwork manager for Windows

> **The product is called Griddle.** A griddle is a hot plate that puts a grid on things; the pun
> lands without needing to be argued for, which is why it won.
>
> **It was checked, not assumed** `[VERIFIED-BOX 2026-07-31]`. Every collision is a developer
> library — `GriddleGriddle/Griddle` (a React grid, 2,488★), two CSS grid frameworks, and
> jonhoo's `griddle` crate (212k downloads) — and **not one is in gaming or game art**. That is
> the axis that decides it: *Sprite* was rejected because its top ten GitHub results were all
> pixel-art tools, in the same room as this app's users, and *Griddler* because "griddlers" means
> nonogram puzzles. Griddle's namesakes live in a world these users never visit.
>
> The bare names `griddle` on crates.io and npm are taken, which costs nothing: `@griddle/shared`
> is a private workspace package that is never published, and the crates are `griddle-core` /
> `griddle-app`.
>
> 🔵 The folder is still `steamdb_loader` — a placeholder, and a doubly wrong one: it's
> SteamGrid**DB** (not SteamDB), and there is no "loader" (the entire point is *not* being Decky
> Loader). Renaming it is a manual step outside a session, since it moves the working directory.

### 🔴 Not every "sgdb" is the old name — most of them mean SteamGridDB

The rename was **not** a find-and-replace, and this is why. These are correct and must stay:

| Keep | Because |
|---|---|
| the `sgdb::` module tree (`sgdb::client`, `sgdb::query`, `sgdb::model`, `sgdb::key`) | It is the SteamGridDB API client. `griddle::client` would be a lie. |
| `SGDB_API_KEY`, `SGDB_STEAM_PATH`, `SGDB_REAL_SHORTCUTS` | User and test contracts naming the service |
| `examples/sgdb_probe.rs` | It probes SteamGridDB |
| `cache::MAGIC` = `b"SGDBCA1\n"`, `ENTRY_EXT` = `"sgdbc"`, `settings::TEMP_SUFFIX` = `".sgdbtmp"` | On-disk format markers. Renaming them only invalidates data, and the cache treats a mismatch as a **miss** — so the churn would be silent. |
| `check-secrets.sh` key patterns | They match the *service's* key format |

🔴 **The trap that nearly fired:** `dpapi::ENTROPY` read `b"sgdb-core:api-key:v1"` and looks like
a crate reference. It is the secondary entropy for `CryptProtectData` — a *format version* — and
its own doc says changing it invalidates every stored key. A blanket `sgdb-core` → `griddle-core`
would have rewritten it, and the failure is silent: everything loads and only the API key comes
back undecryptable, which reads as a key-storage bug rather than as the rename. It was changed
**deliberately**, to `b"griddle:api-key:v1"`, because nothing had shipped and the cost was one
re-entry — the only moment that is ever free.

`APP_DIR_NAME` moved from `SteamGridDB Client` to `Griddle` on the same reasoning, with no
migration code. It is now defined **once**, in `settings`; `cache` re-exports it. The two used to
be separate constants kept in step by a hand-written comment, and a mismatch would have split the
settings and the cache across two directories with nothing to report it.

A Windows-native replacement for the **SteamGridDB Decky Loader plugin**. **One deliverable:**

- **A — desktop GUI.** Lists the Steam library, browses/applies SteamGridDB artwork. 1:1 with
  the Decky plugin's feature set, plus controller navigation.

**The insight the whole design rests on:** the Decky plugin doesn't write files. It calls
`SteamClient.Apps.SetCustomArtworkForApp` from inside Steam's JS realm, which is why art applies
*live*. Every existing Windows tool (Steam Art Manager, SGDBoop, BoilR) writes files and needs a
Steam restart. That realm — `SharedJSContext` — is reachable from a native app over Steam's own
CEF remote-debugging port. So we get Decky's behaviour from a normal Windows app, with no DLL
injection and no Millennium.

### 🔵 Deliverable B — the in-Big-Picture UI — was cut, 2026-07-31

This document argued for it at length and the argument is kept below, because a document that
quietly drops its own reasoning is worse than one that changes its mind out loud. What changed:

**Every mechanism that makes a Decky plugin break on a Steam update was exclusive to B.** Live
apply calls a native CEF binding Valve cannot rename without breaking their own client; the
file-write floor needs no Steam internals at all. Injection needed mangled exports, structural
finders, React context providers and focus trees — the entire fragile surface, in one deliverable
that was **never built**: `apps/bpm` was 99 lines that rendered nothing.

🔴 **The thing that turned "defer" into "delete"** was that Settings → Diagnostics shipped a
*"Check Steam compatibility"* button reporting ✓/✕ on three features — Big Picture UI,
Context-menu entry, Zoom slider — **none of which existed**. A green tick against a capability the
app does not have is worse than no panel. Deferring would have kept shipping it.

The S2 research below is deliberately **not** deleted: the mount recipe, the module ids, the five
wrong turns and the focus-tree proof all stand, and M6 is recoverable from them plus git history.
What is gone is code, not knowledge.

The couch use case B existed for is being served instead by **controller navigation in the desktop
app** — see the plan file. Griddle is added to Steam as a non-Steam shortcut and launched from Big
Picture. 🔴 The pad is read **natively in Rust**, never through the webview's Gamepad API:
[WebView2Feedback #5507](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5507) is an open
bug where **gamepad input dies in WebView2 apps whenever the Steam Overlay is attached**, which is
exactly the launched-from-BPM case, and #3025 is a second open bug where the API only delivers
events while DevTools holds focus. Steam Overlay hooks XInput/DirectInput/RawInput/WGI and injects
an emulated Xbox pad into them, so a native read sees Steam Input's mapping by design — the same
hooking that breaks WebView2's plumbing is what makes the native path work. `[VERIFIED-SOURCE]`

- **Crates:** `griddle-core` (all logic) · `griddle-app` (thin Tauri shell)
- **Packages:** `@griddle/shared` (logic shared with the UI) · `apps/desktop`
- **License:** **Apache-2.0.** `LICENSE` + `NOTICE` at the root, no per-file headers.

### 🔵 It was GPL-3.0-or-later until 2026-08-01, on an argument that was never exercised

The reasoning was that GPL made `decky-steamgriddb`, `@decky/ui` and Steam Art Manager (all GPL)
legally *adaptable* rather than merely readable. Nothing was ever adapted, so the licence was
buying an option this project did not use — and copyleft on a Windows GUI app costs its users
something real. Two audits were run before relicensing, because that is only lawful if no GPL
source was actually copied in:

**Provenance — clean.** All 37 `decky` mentions in source are prose *about* what that plugin does,
in comments. No `Copyright (C)`, no GPL header, no "adapted from" anywhere in the tree. The three
closest calls are all uncopyrightable facts, independently verified here: the `ELibraryAssetType`
ordinals (read from decky-frontend-lib's typings, then **measured** with the 6×2 sweep recorded
below), the default filter set (boolean toggles chosen for parity, behaviour *observed*), and
`DEFAULT_POSITION` (a JSON literal Steam itself defines).

**Dependencies — one weak-copyleft crate ships, and it does not bind us.** 446 crates,
overwhelmingly `MIT OR Apache-2.0`. `r-efi`'s `LGPL-2.1-or-later` is one arm of a triple licence;
we take Apache-2.0. Frontend: 75 packages, all permissive. The only thing needing a decision was
five **MPL-2.0** crates — `cssparser`, `cssparser-macros`, `selectors`, `dtoa-short`, `option-ext`.

🔴 **Four of them are proc-macro-only. `option-ext` is not, and `cargo tree -i` says otherwise if
you read the first path it prints.** That command showed `dirs → tauri-build
[build-dependencies]` at the top, which reads as build-only and is what this document claimed for
a few minutes. There is a **second** path — `tauri → dirs → dirs-sys → option-ext` — and it is
linked into the exe.

**The command that cannot be misread**, because it drops build *and* proc-macro edges:

```powershell
cargo tree -p griddle-app --target x86_64-pc-windows-msvc -e normal,no-proc-macro -i <crate>
```

`"nothing to print"` means it is not in the binary. Four of the five print nothing; `option-ext`
prints the chain. **Run this form, not `-i` alone**, before concluding a dependency is build-only.

That is fine, and is why the licence still works: **MPL-2.0 is file-level copyleft.** §3.3 permits
a Larger Work under any terms provided the MPL-covered *files* stay MPL and their source stays
available, which reproducing the text and the repo URL in `THIRD-PARTY-NOTICES.txt` discharges.
Nothing propagates to Griddle's own source. Full reasoning in `about.toml`, next to the config it
justifies.

**`THIRD-PARTY-NOTICES.txt` is not optional and is unrelated to which licence we pick.** The
release profile statically links every crate into the exe, and MIT/BSD require their copyright
notices travel with the code. Generated by `cargo about generate`, checked in, and regenerated in
CI and diffed — so a new dependency cannot slip in unattributed.

Full plan: `C:\Users\jeff\.claude\plans\i-want-to-start-valiant-shamir.md`

---

## ⚠️ Read this first: the verification discipline

**Every fact in this document is tagged. Do not add an untagged claim.**

| Tag | Meaning |
|---|---|
| `[VERIFIED-BOX]` | Confirmed read-only on this machine, with the date. The strongest tag. |
| `[VERIFIED-BOX @ CLSTAMP n]` | Read out of Steam's shipped JS bundle. **These expire** — Steam rewrites `steamui/` on update. The stamp says which build it was true for. |

> ### 🟢 Steam updated, and the module map was re-resolved against it
>
> Steam went from `10840511` (2026-07-27) to **`10856968`** (2026-07-30) — three days. The
> finders were run against the new build with
> `cargo run -p griddle-core --example cdp_check`: **11/11 resolved, all features available.**
>
> 🔑 **Every module id was unchanged.** `FocusableFactory` 28869 · `ModalManager` 3673 ·
> `ModalHost` 36437 · `AppContextMenu` 5808 · `ShowContextMenu` 39590 · `FocusTreeNode` 4690 ·
> `SteamArtworkFlow` 87498 — all identical to the spike's values, and still 2564 modules with
> 0 unreadable. So webpack module ids in Steam's bundle appear **stable across a build bump**
> for modules that did not themselves change. Do not rely on that (it is one data point), but
> it does mean a stamp change is not automatically a re-resolution emergency.
>
> Newly recorded on `10856968`: `LogoPosition` 78057 · `SliderField` 64608 ·
> `ArtworkApi` 80818, 81659, 87498.
| `[VERIFIED-SOURCE]` | Read in someone's actual source (Valve's bundle, SGDBoop, decky-steamgriddb). Quote it. |
| `[VERIFIED-DOCS]` | Someone's published documentation. Weakest of the three — docs lie, and this project has been bitten by that. **Currently used zero times**, which is the right number: everything load-bearing here was measured instead. |
| `[INFERRED]` | Reasoning, analogy, or a third-party blog. **Must be promoted before it becomes load-bearing.** |

This is not bookkeeping. During the design pass, **the single most widely-repeated fact about
non-Steam shortcuts turned out to be false** — see the CRC32 entry below. It is repeated in
practically every tutorial, and four variants of it were computed against the real file before
concluding it simply does not hold on modern Steam. Had it gone in unverified, every non-Steam
game would have had its artwork written to a filename Steam never reads, and the bug would have
looked like "Steam ignores custom art" rather than "we computed the wrong number."

**Additionally: record the *finder predicate*, not just the conclusion**, for anything read out
of `steamui/`. When a Steam update breaks something, the predicate is what you edit.

### 🔴 The tags were never the problem. The untagged sentences were.

Audited in full on 2026-08-02, after four claims here had already changed the code for the worse.
**Not one of them was a mis-measurement.** Every tagged fact that could be re-checked held. All
four failures were *untagged inferences written in the same confident voice as the measurements
beside them*:

| It said | It was | It cost |
|---|---|---|
| *"writing [an icon] still has no route… stays disabled for Steam apps"* | an inference from decky writing into `librarycache` | a working feature replaced with a paragraph calling it impossible |
| *"icons need… Steam must be shut down; then restart"* | true of the *file*, not of Steam's API | a shutdown/relaunch flow for a problem `SetShortcutIcon` does not have |
| *"four [MPL crates] are proc-macro-only"* | `cargo tree -i`'s first path, mistaken for its only one | a licence audit called clean on a crate that ships |
| *"`librarycache`… contains no write at all"* | true when written, false within weeks | nothing yet — caught by this audit |

🔑 **So the rule to actually follow is narrower than "tag everything":** when you write a sentence
that tells a future reader something is *impossible*, ask what you measured. If the answer is "I
read that someone else does it differently", that is `[INFERRED]`, and this document has just
demonstrated four times over that `[INFERRED]` in the voice of a fact is how good code gets
deleted.

⚠️ **A document cannot verify itself.** All four survived repeated readings because each one was
internally consistent with the paragraph around it. Check claims against the code, the machine,
Steam or the API — never against CLAUDE.md.

`scripts/check-claims.sh` now enforces the grep-able subset, and runs in the gate and in CI. It
covers six invariants and nothing that needs Steam, the network or a key. **Its first run found
two emoji markers in `styles/*.css` that a hand-written grep had missed**, and its CRC32 rule was
itself broken on first writing — `\bcrc32\b` matches neither `crc32_of` nor `crc32_ieee`, so it
passed vacuously until it was fired against a real violation. Fire every rule before trusting it.

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
| `appcache\librarycache\` | **2249** per-appid dirs vs 51 appmanifests — a superset (owned/browsed, not installed). **The filename is not the predicate — `appinfo.vdf` indexes it**, see below. **Read-only. Never write here** — Steam re-downloads over it. |
| `userdata\<id>\config\localconfig.vdf` | 200 KB text VDF. Its `apps` map holds **518** appids — the offline "all games" source. See below. |
| `userdata\<id>\config\librarycache\<appid>.json` | **Achievement data, not art.** Same name, different thing. Do not confuse with the above. |
| `userdata\<id>\config\licensecache` | Encrypted binary. Dead end for an owned-games list. |

#### 🔴 `appcache\librarycache\` is sha1-keyed — earlier notes here were wrong

**This section has now been wrong twice, and the second correction matters more than the first.**

An earlier version said the flat files are "all sha1-named". They are not. Measured across all
**2248** appid directories `[VERIFIED-BOX 2026-07-30]`, **1945** hold semantically *named* files
(`header.jpg` 1856, `library_600x900.jpg` 608, `library_hero.jpg` 594, `logo.png` 526,
`library_hero_blur.jpg` 513, `library_header.jpg` 50) and **278** hold them one level down under
a sha1 directory. Re-measured `[VERIFIED-BOX 2026-08-02]`: **2249** dirs, **1970** flat and
**279** nested — one new app, and the split is unchanged in kind.

⚠️ **`279` is not the same statistic as the `270` wide capsules in the table below**, and
conflating them is an easy mistake to make while updating this section — it was made once during
the 2026-08-02 audit and caught by re-measuring. This counts *directories that contain a
subdirectory*; that counts *one slot resolved through one*. An app can be nested for one slot and
flat for another.

But correcting the census does not give a usable rule, because **the same slot has different
filenames on different apps**. The durable finder lives in `appinfo.vdf`:

> **Predicate: `common/library_assets_full/<slot>/image/<lang>` holds the path *relative to*
> `librarycache/<appid>/`, sha1 component included. Read it, then `is_file()` it.**

```text
620      library_capsule -> "library_600x900.jpg"
1030300  library_capsule -> "93637c34351160eaa7d7ff0cce69cb4312abb819/library_capsule.jpg"
1091500  library_capsule -> { english: "…", schinese: "…/library_capsule_schinese.jpg" }
```

Corroborated structurally: `library_assets_full` occurs exactly **once** in `appinfo.vdf` (a v29
string-table *key*) while `library_capsule` occurs **305×** (inline path *values*).

**Measured resolution**, via `cargo run -p griddle-core --example scan`. Re-run
`[VERIFIED-BOX 2026-08-02]` over **2249** dirs, one more than the 2248 of 2026-07-30:

| Slot | Resolved | of which reachable *only* via the sha1 layout |
|---|---|---|
| Capsule | 715 | **107** |
| Wide Capsule | 2176 | **270** |
| Hero | 702 | **108** |
| Logo | 621 | **95** |
| Icon | 804 | 0 |

⚠️ **These numbers move with the library and that is not a finding.** Every count that changed
did so by exactly one, because one app was added between the two runs; Hero, Logo and Icon are
unchanged. Re-run the harness and re-stamp rather than "correcting" them — treating ordinary
drift as an error would make this table look unreliable when it is not.

That middle column is the point: a basename-only resolver silently misses 107 capsules and 270
wide capsules. It would have looked perfectly correct on whichever app you happened to test.

Two consequences, both enforced in `steam::librarycache`:

- **appinfo runs *ahead of* disk.** Steam records what art an app has before downloading it, so
  24–32 paths per slot pointed at no file. Every rung ends in `is_file()`.
- **The path is untrusted.** It is a string out of a 6 MB binary, joined onto a directory the
  `asset:` scope covers *recursively*. `safe_join` refuses `..`, absolute paths and drive
  letters — the same guard, and the same reasoning, as `cache`.

Bare filenames survive only as a **fallback**, for the ~1570 apps with no `library_assets_full`
entry at all, which have just `header.jpg` and an icon.

This is still why the cache is read-only for us: the naming is a Steam implementation detail
that has now changed at least twice. Custom art goes in `userdata/<id>/config/grid/`, which is
stable and documented by usage.

#### 🔴 The tiny sha1-named `.jpg` is `common/icon` — and `icon` ≠ `clienticon`

The 484–1981 byte sha1-named `.jpg` sitting beside the artwork is **not junk**: it is the small
game icon, named by `common/icon`. 628 of 630 matched, and 620's local `25a5a16b….jpg` is
byte-size identical (1025 B) to
`cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/620/25a5a16b….jpg`.
`[VERIFIED-BOX 2026-07-30]`

🔴 **`common/icon` and `common/clienticon` are different sha1s on the same app** (1030300:
`b4a999c1…` vs `28f5a413…`). The librarycache `.jpg` matches **`icon`**; `clienticon` names a
`.ico` under `Steam\steam\games\` (57 files here). Conflating them yields a path that does not
exist. This is also the only thing that can show an Icon *default* for a real Steam app, which
S8 otherwise left as a dead end.

There is deliberately **no positional fallback** for icons — no "the only sha1 `.jpg` in the
directory". Without `common/icon` there is nothing to match on, and guessing by position is
exactly the coin flip this whole section exists to avoid.

#### 🟢 Steam's artwork CDN — its own fixed name table `[VERIFIED-BOX 2026-07-30]`

The last rung before a placeholder, and the only source that covers not-installed games. Base
`https://shared.steamstatic.com/store_item_assets/steam/apps/<appid>/<name>`; mirror
`https://cdn.cloudflare.steamstatic.com/steam/apps/<appid>/<name>`.

| Slot | CDN name | 620 | 1030300 |
|---|---|---|---|
| Capsule | `library_600x900.jpg` | 200 | **200** — its disk name is `<sha1>/library_capsule.jpg` |
| Header | `header.jpg` | 200 | 200 |
| Hero | `library_hero.jpg` | 200 | 200 |
| Logo | `logo.png` | 200 | 200 |
| — | `library_capsule.jpg` | **404** | **404** |
| — | `library_header.jpg` | — | **404** |

🔴 **The CDN name is not the disk name.** The two 404 rows are recorded deliberately: they are
what stops someone "fixing" the table by copying names out of `librarycache`. A test asserts
`library_capsule.jpg` appears nowhere in it.

Icons are a different host *and* path:
`cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/<appid>/<common/icon>.jpg`.

Shortcut appid `4048848997` → 404, so non-Steam entries need no special case beyond
`kind !== 'steam'`. Both hosts were already in the CSP.

#### 🟢 `localconfig.vdf` — the offline "all games" source `[VERIFIED-BOX 2026-07-30]`

`userdata/<id>/config/localconfig.vdf`, 200 KB of **text** VDF, so `vdf::text` reads it — no new
codec. Key path `UserLocalConfigStore` → `Software` → `Valve` → `Steam` → `apps`.

> **Predicate: a child is a Steam app iff its key parses as `u32` and its value is a map.**
> 519 children, **0 scalar siblings**.

**518 appids against 51 `appmanifest` files.** There is still no *offline* ownership list —
`licensecache` is encrypted — so this is a proxy, not a license check: it misses games owned and
never launched. The UI says "All games", never "owned".

#### 🟢 How much it misses, measured — and the live list that fixes it `[VERIFIED-BOX 2026-08-02]`

*"Misses games owned and never launched"* went unquantified for a milestone. It is **391 apps**,
about **200 of them real games**. Read-only over CDP against the running client:

| Source | Apps | of which `app_type` Game |
|---|---|---|
| `localconfig.vdf` `apps` map | 519 | ~469 after filtering |
| `collectionStore.allAppsCollection.allApps` | **869** | **667** |
| `appStore.allApps` | 869 | — |
| `collectionStore.localGamesCollection` | **51** | — |

That last row is the control: 51 exactly matches the 51 `appmanifest` files, so the probe is
reading what it claims to.

**The 391 the live list adds are precisely the predicted gap** — owned, never launched here:
Psychonauts, Crysis Wars, Vermintide, Star Trek: Bridge Crew, Quantum Break.

🔑 **The 41 going the other way are not losses, and one of them is a measurement artifact.** The
EmulationStationDE shortcut is in *both*; `localconfig` stores the signed `-246118299` and the JS
realm the unsigned `4048848997`, so a naive diff counts it as missing from each.

🔴 **What this section said about the remaining 40 was wrong, and wrong in the way this file exists
to prevent.** It read: *"The rest are the refunded-and-withdrawn apps `is_disowned` already drops —
that inference now has independent corroboration."* **Nothing was checked.** The 40 were never
opened; "the same apps we already drop" was an assumption dressed as a result, written in the same
paragraph as four real measurements and borrowing their credibility.

It was falsified within the hour, by the library's owner rather than by a probe: **six** of those
40 were apps `is_disowned` does *not* drop, Assassin's Creed Shadows among them. Far from
corroborating the appinfo heuristic, the live list **disagrees with it** — which is precisely what
makes it worth having.

⚠️ The lesson is the one already at the top of this file, and it still caught me: *an inference
sitting next to a proven fact borrows its credibility.* The tell was available — "the rest are X"
names a set nobody enumerated. **If a sentence describes a set, print the set.**

`app_type` arrives as Steam's `EAppType` bitfield rather than `appinfo`'s inconsistently-cased
`common/type` string: Game 667 · Tool 181 · Music 10 · Application 6 · Beta 3 · Video 1 ·
Shortcut 1.

🔴 **There is still no licence list anywhere.** No `LicenseStore` global, and `SteamClient.Apps`
exposes nothing matching owned/licence/library beyond `ReportLibraryAssetCacheMiss`. 869 is "what
Steam's library shows", which is much closer to ownership than `localconfig` and still is not it.
**The UI must keep saying "All games".**

⚠️ **These are Steam's own globals, not a CEF-host binding**, so they sit in the middle of the
risk scale this project cares about: not minified module internals, but not `SetCustomArtworkForApp`
either.

#### 🟢 Shipped as a fourth rung, 2026-08-02 — **479 → 683 games**

`commands::library` is now a ladder, and the order is the point: the three file-based rungs run
first because they are cheap and always available, then `merge_live_apps` fills the gaps.

| Rung | Source | Needs Steam running |
|---|---|---|
| 1 | `appmanifest_*.acf` | no |
| 2 | `localconfig.vdf` — and the **only** source of playtimes | no |
| 3 | `shortcuts.vdf` | no |
| 4 | `collectionStore` over CDP, All-games scope only | **yes** |

Measured on the real client: `live=869 added=204 skipped_type=192`, and the list went from **479
to 683**. That `192` is the arithmetic checking out — 181 Tools + 10 Music + 1 Video, exactly the
type census above. Cost is **33 ms** end to end, from the log timestamps either side of it.

🔑 **`AppType::from_steam_enum` exists so there is one policy, not two.** The live list carries a
numeric `EAppType` and `appinfo.vdf` carries an inconsistently-cased string; both map into the same
enum and through the same `belongs_in_library`. Two filters would eventually disagree about whether
a dedicated server is a game, and only one of them would be tested.

🔴 **Every failure in rung 4 is silent, deliberately.** Steam being closed is the *ordinary* case,
so a connection error, an absent store, or a moved global all leave the list exactly as rungs 1–3
built it. Only the middle case warns: reaching Steam and not finding `collectionStore` means a
build moved something, which is worth a log line. "Steam is not running" is `debug`, because
warning about it on every library load would be noise about a thing nobody asked for.

#### 🟢 It prunes as well as adds — **683 → 677**, and the six are named in the log

Adding was only half of it. The same fetch answers "what does this account hold?", so a row
`localconfig` remembers that Steam does **not** list is one the account no longer holds. Measured:
`pruned=6`, and every one is recognisable as such — **Assassin's Creed Shadows** (the refund that
started this), Marvel Heroes Omega (delisted), two withdrawn demos, a withdrawn beta, and an
unreleased pre-order.

🔴 **`installed` wins outright and is never pruned.** Files on disk are a fact; Steam listing
everything the account holds is an assumption. If those ever disagree — mid-sync, offline mode, a
family-sharing quirk — showing a game the user can launch beats hiding it. `keep_after_live_reconcile`
is the predicate, deliberately extracted from the async path so both directions are unit-testable,
each with a control.

⚠️ **The prune keys on the *unfiltered* live ids**, before the type filter. A Tool the account owns
is still owned; keying on the filtered set would drop rows for the wrong reason and the log would
blame ownership.

🔑 **The log names the titles, it does not just count them.** This removes rows the user can see,
so "which ones?" has to be answerable without re-deriving it — and a name is how anyone spots the
day it starts eating games that are genuinely owned.

Three things it deliberately does not do, each of which would be a bug:

- **Never replaces a row.** Rungs 1–2 carry installed state and playtimes that this list does not;
  clobbering them to gain a `display_name` trades information for nothing.
- **Never adds a shortcut.** Steam types those `1073741824` and `shortcuts.vdf` owns them. Adding
  one here produces a duplicate row under the *unsigned* appid.
- **Never relaxes the blocklist.** `is_known_non_game` is still the floor under both paths.

🔴 **The 519th key is `-246118299`** — the signed form of `0xF1548865`, the EmulationStationDE
shortcut. `shortcuts.vdf` owns those, so it is skipped; taking it too would duplicate the row
under a different name. `u32::from_str` refuses it for free, which is why there is no sign check.

🔴 **`LastPlayed` has a sentinel that is not a date.** Eight entries read `86400` (1970-01-02,
33 years before Steam existed) and six read `0`. Anything at or below `STEAM_LAUNCH_EPOCH`
(`1_063_324_800`) is reported as never-played, or "recently played" opens in 1970.

Against `appinfo.vdf`: 469 typed `Game` (**409 `Game` + 60 `game`** — the casing really is
inconsistent, and `AppType::parse` already handles it), 10 `Tool`/`Config` which get filtered
out, and **29 absent from appinfo entirely with no cache dir** — delisted. They are still shown,
following the fixed failure direction: a Steam appid is still a valid SteamGridDB key, and a
missing game is a bug report while an odd-looking row is a cosmetic annoyance.

#### 🔴 Absence from `appinfo.vdf` means the account no longer holds the app — drop it

They are **not** delisted, which is what this section said first. The library's owner identified
them: **refunded purchases, plus demos and betas Steam has since withdrawn.** The list bears that
out — a refunded Mortal Kombat 1 and Black Ops III, three Resident Evil demos, a Division beta.

`localconfig.vdf` records what was *configured*, never what is *owned* — there is no offline
ownership list at all, `licensecache` being encrypted — so an app it remembers that `appinfo.vdf`
has never heard of is the closest thing to a "no longer yours" signal available **offline**.

🔴 **This section used to end "Steam drops an app from appinfo once it stops being yours." That is
false, and it was never measured.** `[VERIFIED-BOX 2026-08-02]` The library's owner spotted a
refunded **Assassin's Creed Shadows** back in the list. It is in `localconfig.vdf`, and
`appinfo.vdf` **carries its name**, so `is_disowned` keeps it — it had been showing the whole time.
`appinfo.vdf` is a *global metadata cache*, not a per-account one, and nothing obliges Steam to
evict from it.

So `is_disowned` is a **weak heuristic that catches some of them**, not a rule. It stays, because
offline it is all there is. What it is not is correct.

🔑 **The live list settles it properly, and that is now the primary predicate.** Steam does not
list a refunded game in your library, so when `library_apps` succeeds, *absence from it* is the
ownership signal — see the reconcile below, which caught six on this box including the one that
started this.

**This reverses the usual "unknown means show it" direction, deliberately.** These are not games
missing from the list; they are games no longer in the account, and every one is unnamed and
artless. `is_disowned` in `commands.rs` is the predicate, with a `tracing::info!` counter as the
tripwire.

🔴 **`appinfo_loaded` is the whole safety story, and it is a parameter rather than something the
predicate works out.** With no readable `appinfo.vdf` *every* app looks nameless, so dropping on
namelessness alone would cut the All-games scope down to installed apps and shortcuts — surfacing
as "some of my games are missing", the hardest kind of bug to report. **No appinfo means no
opinion.** Two tests, one per direction, each with a control.

Re-measured when they were reported as "the title and art don't load" `[VERIFIED-BOX 2026-07-31]`:

| Question | Answer |
|---|---|
| Present in `appinfo.vdf` but unnamed? | **0** — all 30 are absent *entirely*, so no parser fix can help |
| Have a `librarycache` directory? | **0** |
| Keys in their `localconfig` entry | `LastPlayed`, `Playtime` — **no name** |
| Steam CDN still serves a capsule | **18 of 29** |
| Known to SteamGridDB by appid | **14 of 29** — incl. Mortal Kombat 1, Elite Dangerous, Black Ops III |

The 30th is `4048848997`, the EmulationStationDE shortcut, which is named from `shortcuts.vdf`.

That 14-of-29 row is why the first fix — relabelling them — was the wrong one: some of these are
perfectly recognisable games, so no label was ever going to make the row look right. Dropping them
is what the data supports.

`LibraryEntry::named` survives as a **degraded-mode signal only**: once disowned apps are dropped,
the only unnamed rows left are those from an unreadable `appinfo.vdf` and a shortcut with no
`appname`. That is exactly when the UI most needs to explain itself, so the row says *"Steam has
no details for this app"* rather than showing a synthesised name with nothing to back it.

**It was never throttling, and never scroll speed.** The rows are deterministic and appear only
under the **All games** scope. A burst of 200 concurrent Steam CDN requests measured 200/200 OK in
0.34 s, so nothing here is rate-limited. Both of the first two explanations — a CDN failure under
burst, then a bad label — were wrong, and only measuring killed them.

#### 🟡 `logo_position` is already in `appinfo.vdf` — free for the logo positioner

`common/library_assets_full/library_logo/logo_position` carries Steam's own default, which the
positioner would otherwise have to invent:

```text
440:     { pinned_position: "BottomLeft", width_pct: 26,                  height_pct: 37 }
1030300: { pinned_position: "BottomLeft", width_pct: "51.53499168364688", height_pct: "79.538…" }
```

🔴 **`width_pct`/`height_pct` are sometimes `T_INT32` and sometimes `T_STRING` in the same
file.** A reader that assumes either one skips half the apps. Not captured yet — recorded so it
is not re-derived later.

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
`griddle-core` contains no CRC32 function at all, so there is nothing to regress to. Only use a
generated id when *creating* a brand-new shortcut, where Steam honours whatever we wrote.

### Steam's JS surface `[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]`

> 🟢 **Re-checked on `10856968`, 2026-08-02**, with `cargo run -p griddle-core --example
> cdp_check`: `SharedJSContext` found, the apply API present, and the page and
> `steamui/changelist.txt` agreeing on the stamp. The rows below still carry the `10840511`
> stamp because that is the build their *literal chunk filenames* were read on — the facts hold,
> the file names are the part that expires.

| Fact | Finder / evidence |
|---|---|
| Build stamp is readable from **both** disk and the live page | `steamui\library.js` line 1 is `var CLSTAMP="10840511";` and `steamui\changelist.txt` contains exactly `10840511` |
| Module-discovery hook exists | `window.webpackChunksteamui` present in `library.js` |
| The apply API exists **and Valve hardcodes the mime** | `steamui\chunk~2dcc5aaf7.js` contains `SetCustomArtworkForApp(e,r,"png",t)` — Valve's own code passes literal `"png"` regardless of the actual bytes. This is why animated WebP written as `<appid>p.png` animates: Chromium sniffs content, not extension. |
| Logo position payload shape | same chunk: `SetCustomLogoPositionForApp(e.appid,JSON.stringify({nVersion:1,logoPosition:t}))` |
| 🔴 **Name-based module lookup is impossible** | Asset-type enum members appear only as mangled exports (`c.VYj`, `c.JoK`, `c.KoM`, `c.n4o`, `c.b_A`). Every finder must be **structural** — shape, value, or localization-token anchored. |

### Baseline environment `[VERIFIED-BOX 2026-07-27]`

🔴 **This is the "before" snapshot and two of its lines are now false *because this app made them
false*.** Read it as history, not as the current machine.

- Port 8080: **no listener**. `.cef-enable-remote-debugging`: **absent**. No proxy `user32.dll`
  → Millennium genuinely not installed. Clean slate.
- Toolchain: Rust 1.97.0 (MSVC only), Python 3.11, git 2.54, **bun 1.3.14** (installed by this
  project — was absent). No Node/npm.
- Steam running as pid 15844 with 7 `steamwebhelper` children.

**Now** `[VERIFIED-BOX 2026-08-02]`: the sentinel is **present** and 8080 **has a listener**, both
created by Griddle at startup — that inversion is the feature working, and the two lines above
are what it looked like beforehand. The toolchain is unchanged: `rustc 1.97.0`, `bun 1.3.14`,
`git 2.54.0`, `Python 3.11.0`, still no Node. Steam's pid moves every restart and was never worth
recording; it is kept only so the shape of the process tree is on file.

---

## The rules

### No silent failure

Workspace lints make these guarantees, not preferences: `unused_must_use`,
`let_underscore_must_use`, `unwrap_used`, `expect_used` are all **deny**. An ignored `Result`
must not compile, and `let _ = ...` is a build failure. `-D warnings` in CI.

`thiserror` in core — the UI must distinguish "Steam is running, can't write shortcuts" from
"network timeout". `anyhow` only in `griddle-app`.

### The write boundary (CI-enforced)

**Only `grid::store`, `steam::shortcuts`, `settings` and `cache` may write files.** Every other
`fs::write|File::create|remove_file|remove_dir|OpenOptions` fails CI unless the line carries a
`boundary-ok:` annotation — on the same line, or on the line directly above.

`cache` is on the list for a different reason from the other three: it writes only under
`%LOCALAPPDATA%\<app>\cache`, which we created and which is disposable. The boundary exists to
keep writes to the user's *irreplaceable Steam config* auditable, and a cache we can delete at
will is not that. Every path there derives from its own root and every filename is a hash, with
a test asserting a key like `../../../../windows/system32/evil` cannot escape.

🔴 **The check used to stop at the first `#[cfg(test)]`**, on the theory that tests come last
and legitimately write fixtures into tempdirs. That is a heuristic about *layout*: a write
placed **after** the test module was invisible, demonstrated by appending `std::fs::write` to
the end of `appid.rs` and watching the check pass. Parsing Rust well enough to identify test
code is not worth it — `#[cfg(test)]` sits on modules, functions, *and* single struct fields,
and brace-counting breaks on the last. Now every line is scanned and the three legitimate
fixture writes are annotated. Verified against five cases, including "annotation two lines
above must not exempt".

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

### 🔑 Live apply is set up, not offered — a reversal, on purpose

The sentinel used to be behind an opt-in checkbox, and this document used to say *"enabling a
debugging port on someone's machine without asking is not ours to do."* That was the wrong call
for this product and it is now created at startup.

The reasoning that changed it: **applying artwork without restarting Steam is the entire reason
this app exists** in preference to Steam Art Manager or SGDBoop. Putting its one prerequisite
behind a checkbox meant the product shipped switched off for anyone who never found it — the
headline feature, off by default, in a settings tab. CSS Loader and Decky set the identical flag
and mention it to no one.

🔵 **The first-run disclosure has since been removed too, on the maintainer's call.** For a while
the setup screen said what the file was, that it is Valve's own setting, that CSS Loader and Decky
use it too, and that deleting it undoes everything. That panel is gone; what remains is the
**Live apply** row in Settings → Diagnostics, which reports the sentinel's state through
`Status::sentinel_explanation`.

So the flag is now created silently and explained only if the user goes looking — which is exactly
what CSS Loader and Decky do, rather than more than they do. Recorded plainly because the section
above argued the opposite case, and a document that quietly drops its own reasoning is worse than
one that changes its mind out loud.

Be clear-eyed about what it costs: Steam opens its CEF debugging port on loopback at next start,
so any process already running as this user can drive Steam's JS. Modest, Valve's own mechanism,
and the same exposure those tools have always carried — but real, and it belongs in the copy
rather than in a footnote.

Consequences in code: `Settings.live_apply` is **gone** rather than defaulted to `true` (a
stored `false` would have stranded someone in file-mode with no UI to change it), the
`set_live_apply` and `remove_sentinel` commands are gone, and `apply_asset`/`clear_asset` decide
by *capability* — `AssetType::supports_live_apply` — never by preference. `enable()` runs on
every launch because it is idempotent and never truncates, which also repairs the file if
something removed it; Millennium is known to.

The file-write path stays as the floor of the ladder. It is what makes this shippable if Steam
moves the API, and it needs no port at all.

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
| **M2** | **Offline layer done**, including the `shortcuts.vdf` writer. Verified against the real install with `cargo run -p griddle-core --example scan`. |
| **M3** | 🟢 **The app runs.** Library list with current art, five asset tabs, SteamGridDB browsing with infinite scroll, apply with the live→file ladder, first-run key flow, and a diagnostics screen. |
| **M4** | 🟢 **Default art, library scope, and filter parity.** Steam's own artwork behind the custom art (local cache → CDN → placeholder); an Installed / All games toggle with sorting; the full SteamGridDB filter set wired through; and the "wrong game?" picker. The asset tabs now render only inside a game. |
| **M5** | 🟢 **Controller navigation, and the three M4 leftovers.** Spatial focus grid + native gamepad read; the **asset details modal** (right-click or **Y** for full-size art, author, size, format, style, votes, and a link to its SteamGridDB page); the **tile-size control** on all seven grids, persisted per grid; and the **logo positioner** (Current tab → right-click the logo). Applying from the grid is unchanged — the modal applies nothing until asked. |
| **M6** | 🔵 **Cut.** The Big Picture UI is not being built — see the header. `apps/bpm`, `cdp::modules` and the eleven finders are deleted; the research stays. |
| **M7** | 🟢 **The non-Steam icon flow**, which closed the last M4 item. Icons for Steam games were always an ordinary file write and still are; a *shortcut* additionally needs its `shortcuts.vdf` entry repointed, done through `SteamClient.Apps.SetShortcutIcon` with Steam up and by editing the file with Steam down. Griddle never closes Steam. Every icon needs a restart to show, whichever route ran. |
| **M8** | 🟢 **The first-run screen** — `views/Welcome.tsx`, replacing twelve lines inlined in `App.tsx` that showed the *Settings* key panel under a second heading. Task before policy: what the app is, then the four steps for getting a key (word-for-word the ones in `docs/start/your-api-key.mdx`), then an **Open SteamGridDB** button, then the field. Deliberately **not** a wizard — see below. |
| **M9** | 🟢 **The live library merge.** All-games was a `localconfig.vdf` proxy that missed every game owned and never launched here; with Steam up, `collectionStore` fills them in. **479 → 683 games**, one shared type policy, and silent degradation to the offline list when Steam is closed. |
| **Next** | No feature work outstanding. What remains is release mechanics — a real `v0.1.0-rc.1` tag run, installing the NSIS bundle, and the clean-machine docs walkthrough — plus the undecided **Experimental library tweaks**, which would need the module discovery that went with M6. |

**The M4 changes worth remembering**, all detailed above: `librarycache` is indexed by
`appinfo.vdf`, not by filename; the CDN has its own name table that is *not* the disk name;
`localconfig.vdf` is the "all games" source and contains one negative key that is a shortcut;
and the `asset:` scope now covers a second directory **recursively**.

#### 🔴 Four things the first-run screen settled, and one of them is a general trap

**1. `has_api_key` means ciphertext is present, not that a key works.** DPAPI seals to one Windows
account, so a settings file carried from another PC leaves `has_api_key: true` with
`state.sgdb == None` — and `AppState::load` only `warn!`s about it, which reaches nobody in a
`windows_subsystem = "windows"` binary. The gate is therefore
`!status.has_api_key || status.key_unreadable`, where `key_unreadable` is
`has_api_key() && state.sgdb.lock().await.is_none()` — exactly "stored and unusable", from state
that already existed. The docs had *promised* this was reported since before it was true.

**2. An error message must not name a screen the reader may not have.** `Error::Unauthorized` read
*"…Check it in Settings."*, which is a dead end on first run — the nav bar renders only in the
post-key branch — and redundant once you are in Settings. The message now states the fact and
`UiError`'s `action` carries the remedy, which is the caller's to know. Tests assert the *absence*
of "Settings" in both, so the copy cannot drift back.

**3. Validation gets its own `Config`.** Browsing wants the patient defaults (20 s, 3 retries,
backoff). A human watching a button labelled "Checking…" does not: offline, the defaults take over
a minute to conclude the obvious. `commands::apikey` validates through
`Client::with_config(…, timeout: 8s, max_retries: 1)` and stores a client built the ordinary way.
One retry rather than none, so a single dropped packet does not read as a bad key.

🔑 **4. `autoFocus` cannot seed the focus model, and neither can `.focus()` from an effect.** This
generalises well beyond this screen. The model learns about focus from a `focusin` listener that
looks the element up in the registry, and **both** obvious spellings lose a race with it:

| Spelling | Why it fails |
|---|---|
| React's `autoFocus` prop | Applied during commit, *before* any passive effect — the control is not registered yet, so the lookup finds nothing |
| `.focus()` in a `useEffect` | React runs **child effects before parent effects**, so the provider is an ancestor and has not installed the listener yet. Nothing is listening at all |

`queueMicrotask(() => ref.current?.focus())` runs after the whole effect pass, by which point both
hold. The failure is near-invisible — the caret blinks in the field either way, so it *looks*
focused, and only the first D-pad press reveals the cursor was never set. `Welcome.test.tsx`
asserts the model's own `.focused` class rather than `document.activeElement`, because only the
former can tell the two apart; it was written first and **failed against the `autoFocus` version**,
which is how this was found rather than shipped.

🟢 **Captured 2026-08-02 as `docs/src/assets/welcome.png`, via `screenshots.ps1 -Welcome`.** This
section previously said it could not be done without a throwaway Windows profile. That was wrong
about the *goal* while being right about the danger: what must never happen is a script clearing
a DPAPI-sealed key to reach first run, and the conclusion drawn from it — "so it cannot be
captured" — did not follow.

🔑 **The resolution was to make the state a precondition instead of an action**, the same move that
fixed the "Reset filters" click. The maintainer removes the key in the app, runs `-Welcome`, and
pastes it back; the script asserts that no key is stored and **refuses to run** otherwise. It
touches `settings.json` in neither mode. The person who can undo the state change is the one making
it, which is exactly the property every automated attempt at this lacked.

🔴 **`-Welcome` also forced a guard the main mode had always needed.** With no key stored, the four
main captures are all unreachable — every click lands on a welcome screen that has none of those
controls — so the run would have "succeeded" and overwritten `library`, `browse`, `current` and
`settings` with four copies of the welcome screen. The main mode now asserts a key *is* present.
Fired against the real condition before being trusted, which is how the missing guard was noticed
at all.

The welcome capture synthesises **no input whatsoever** — the app opens on that screen — so there
is nothing to mis-aim and nothing that can land on a control.

### 🟢 The logo, and why the background removal is a flood fill

**Two sources, one script.** `assets/griddle-logo.jpg` is the wordmark — the app header, the
welcome screen, the docs nav and the README. `assets/griddle-icon.jpg` is the "G" mark — the
`.ico` and the docs favicon. `scripts/logo.ps1` derives everything from them, so the set cannot
drift and a new size is one line. It uses `System.Drawing`, like `screenshots.ps1`; neither
ImageMagick nor Pillow is installed and adding one for a script that runs once a year is not
worth it.

🔑 **Two sources is the whole fix.** The first artwork stacked its wordmark under a mascot badge,
so a header wanted a wide crop and an icon a square one, and both had to come out of one picture.
That failed — the word was drawn *over* the badge's lower third, so the badge had no clean edge to
cut along, and every attempt showed the seams. Purpose-drawn art per shape means **nothing here
crops or rearranges anything**: both images are used whole. The four failed attempts and what each
produced are in git history; the short version is that a circular mask rendered "riddl" in a
circle, and cutting the badge free left a raw cross-section along the bottom.

#### The background removal

Both grounds come off by flooding **inward from the border**, never by matching a colour — but
for opposite reasons, so the mode is a property of the source rather than a global.

| Source | Ground | Why it is awkward |
|---|---|---|
| logo | white | 🔴 **The lettering is white too** — 252.9 against 254.1, a 1.2 separation. No threshold can tell them apart. The word survives because the splash **encloses** it and the fill cannot reach in. |
| icon | dark charcoal | A soft vignette is baked around the mark. It is background, but sits well above the ground in luminance. |

🔴 **That logo row is the one to remember.** The correctness of the cut-out does not rest on any
number — it rests on the splash being closed. If a future source lets the ground touch a letter,
the word hollows out silently. **Check the cut-out, not the thumbnail**, and check it on a
checkerboard: a white halo is invisible against white and a dark one against dark.

Three settings, each of which was got wrong first:

- **Thresholds come from a percentile of the border ring (15th / 85th), never the extreme.**
  Deriving from the minimum put one candidate at `lo=-26, hi=16` and keyed the entire image away,
  because a handful of very dark pixels survived in its border. One outlier must not set the
  threshold for the other 99%.
- **The light-mode ramp runs the opposite way to the dark one.** With `hi` above the ground's
  luminance, every background pixel comes out at alpha 0.2 instead of 0 — nothing is removed, and
  the "content bounds" then span the whole frame, which is what the symptom looks like.
- **An alpha floor at 0.11**, or the icon's vignette renders as a dark rectangular smudge behind
  the mark. Real watercolour splatter sits well above it.

Edge pixels are then **un-mixed** — `true = (observed - bg*(1-a)) / a` — because a semi-transparent
pixel is part background, and leaving that in is what haloes the artwork on a different ground.

#### Sizes

🔵 **Posterising to 32 levels is a size fix, and here it needed checking rather than assuming.**
The previous artwork was flat pixel art, where snapping colours is nearly free. Watercolour is
continuous gradient everywhere, so banding was the real risk — but a 3× zoom through the wash
behind the letters is indistinguishable at 64 and at 32, because the paint texture dithers the
steps. The wordmark goes from 630 KB to 122 KB. Alpha is excluded, or the soft rim comes back as
a hard edge.

The app copy is **440px wide**, which is 2.2× the 200px the welcome screen draws it at; the docs
copy stays native, since Astro re-encodes to webp and derives its own sizes. Nothing is ever
resampled *up* — an earlier revision wrote a 482px source out at 640 and the extra pixels were
invented.

⚠️ **`Icon.ToBitmap()` cannot verify a PNG-compressed `.ico`.** It reported non-zero alpha in
corners that are empty and decoded the 256 frame as 128×128 — both wrong. Decode each embedded PNG
payload directly instead. The same lesson as the focus-tree probes: when a probe reports something
broken, check the probe first.

🟢 **Re-captured 2026-08-02, and `screenshots.ps1` needed more than a re-run — as predicted.** All
four (`library`, `browse`, `current`, `settings`) now show the wordmark, the new taskbar icon, no
tagline, and Sort as buttons. Every click target was re-derived from fresh captures; the docs build
is clean and `settings.png` still stops above the Diagnostics rows, which is what keeps the Steam
account id unpublished.

🔴 **One of its clicks had become capable of applying artwork to the maintainer's library.** The
script pressed *"Reset filters"* at fixed coordinates *before* expanding the panel — but that
button renders only when the filters **are** modified (`{modified && …}`), and the panel is seeded
open on the same condition. So on a machine already at defaults, which is the normal case, the
panel was shut, the button did not exist, and the click went to coordinates that the taller header
had moved into the artwork grid. **Clicking artwork applies it.**

The fix is not better coordinates, it is not clicking: default filters are now a **precondition**
asserted against `settings.json` (`filters` must be `null`), and the script exits telling the
maintainer to press the button themselves. That also removed the script's only write to settings.
A normalisation step that can silently rewrite a game's capsule is worse than the unnormalised
screenshot it was preventing.

The header changed height four times, so any claim about its click targets has to say *which*
version it means. Measuring the header's own rule, all at 100/100 samples:

| Header | Rule at | Basis |
|---|---|---|
| 44px horizontal lockup + tagline | y=180 | **unrecorded** |
| 72px mascot + tagline | y=232 | **unrecorded** |
| 72px mascot, no tagline | y=188 | **unrecorded** |
| ~~64px wordmark, no tagline~~ | ~~y=178~~ | superseded — the header is 84px now |
| **84px wordmark, no tagline — what ships** | **y=165** | **client**, `[VERIFIED-BOX 2026-08-02]` |

🔴 **The four older rows do not say which origin they were measured from, and that makes them
unusable rather than merely old.** `screenshots.ps1` clicks through `ClientToScreen`, so its
coordinates exclude the title bar; a capture taken from `DwmGetWindowAttribute`'s frame bounds
includes it, and on this box that is a **38px** difference — larger than the whole spread of the
table. So "y=178" could mean 178 or 140 and there is no way to tell which. The new row states its
basis, and any replacement must too.

The re-measurement is `scratchpad/measure-header.ps1`: it prints the frame origin, the client
origin *and* the delta, then scans a column for the first row matching `--line` (`#2c2f3d`) and
reports the hit in both. It found rgb(44,47,61) at image y=203, client y=165 — the exact token
value, so it matched the rule and not a panel edge.

🔑 **A 2px drift is small enough to look like it still works and not be**, which is the dangerous
size: the script's own header records that a mis-aimed click fails *silently*, producing a
duplicate of the previous image rather than an error. Re-derive the coordinates from a fresh
capture and check that successive captures actually differ. It also resets the filter panel as it
goes, which writes to `settings.json`.

⚠️ **A harness that hides `settings.json` to reach first run must COPY it back, not move it.** An
earlier version moved the file aside and the maintainer's API key was lost when a cycle went
wrong — DPAPI-sealed, so unrecoverable from anything on disk. Keep the backup until the key has
been confirmed working, not merely until the file is back.

🔴 **That rule is necessary and it is not sufficient — the fixed version destroyed the key a
second time, 2026-08-02, by exactly the mechanism it introduced.** "Keep the backup" leaves a
`settings.json.held` lying around, and the restore was guarded by `if (Test-Path $held)`. A *later*
run — not even a first-run one, so it created no backup at all — reached its `finally`, found the
**stale** `.held` from the previous session, and copied that keyless file over live settings that
were newer and had a working key in them.

Both halves looked right in isolation, which is why this got through: keeping the backup is
correct, and restoring it in a `finally` is correct. What is wrong is deciding *whether this run
has a backup* by asking the filesystem, because the filesystem cannot distinguish this run's
backup from last week's.

**The rule that actually holds: a restore must be gated on a flag the same run set** (`$heldByThisRun`),
never on the backup's existence — and a run that is *about* to create one must refuse to start if a
stale backup is already there, since at that moment it cannot tell them apart either.

🔑 **The tell was in the screenshots and was nearly missed.** Capture one showed the full library
— which requires `has_api_key` — and capture two, forty seconds later, showed the welcome screen.
Two images that disagreed about a fact that cannot change on its own. **When two captures of the
same session disagree, something between them wrote to disk.**

### Running it

| | Command | What it does |
|---|---|---|
| **Dev** | `bun run app` | Vite + the app, hot reload. |
| **Real** | `bun run app:release`, then `target\release\griddle-app.exe` | Frontend embedded in the exe. No dev server. |
| Installer | `bun run app:build` | NSIS bundle. |

#### 🔴 `cargo build --release` alone embeds a STALE frontend

`cargo build` does not run `beforeBuildCommand` — only `tauri build` does. So a bare
`cargo build --release -p griddle-app` happily embeds whatever is in `apps/desktop/dist` from the
last time anything wrote there, which may be several milestones old.

This bites *precisely* when the Rust side changed too, because everything looks right: the build
succeeds, the app starts, the window titles correctly, and the dev-server tripwire below
correctly reports that it is serving the embedded frontend. It is serving the embedded frontend —
just last month's. Caught here by comparing `apps/desktop/dist` mtimes against `apps/desktop/src`:
dist was 3 hours older than the sources it supposedly contained.

`bun run app:release` is `bun run build:desktop && cargo build --release -p griddle-app`, in that
order, and exists so the obvious command is the correct one. Same reasoning as putting
`custom-protocol` in `default` below.

**The check, when in doubt:**
```powershell
(Get-ChildItem apps\desktop\dist -Recurse -File | Sort LastWriteTime -Desc)[0].LastWriteTime
(Get-ChildItem apps\desktop\src,packages\shared\src -Recurse -File | Sort LastWriteTime -Desc)[0].LastWriteTime
# dist older than src -> the exe has a stale UI
```

#### 🔴 Even *linting* `griddle-app` needs a built frontend

`tauri::generate_context!` reads `frontendDist` at **compile time** and panics if the directory
is missing, so `cargo clippy -p griddle-app` fails on a tree that has never built the UI — which
is every fresh clone, and was CI's Windows leg for its whole life. The panic reads
*"the `frontendDist` configuration is set to `"../../apps/desktop/dist"` but this path doesn't
exist"*, which sounds like a wrong path in `tauri.conf.json` rather than a missing build step.

The sibling of the trap above, from the other direction: `cargo build` will happily embed a
**stale** dist, and `cargo clippy` will not run at all without **some** dist. CI now builds the
frontend before either.

#### 🔴 The notices check failed on every platform, and it was `core.autocrlf` the whole time

**This section first said the cause "was not chased down" and blamed the runner's OS. That was
wrong**, and the wrong diagnosis was load-bearing: it moved the job to `windows-latest`, where it
promptly failed again with completely different numbers, which is what forced a real look.

`THIRD-PARTY-NOTICES.txt` is a generated artifact compared **byte-for-byte**, and Git for Windows
ships `core.autocrlf = true`. Two independent consequences, neither of which names line endings
when it fires:

| | Effect |
|---|---|
| The generated file carries **65 literal CR bytes** — inside the licence texts of crates whose own `LICENSE` files are CRLF | Git strips them on commit, so the committed file is 65 bytes shorter than anything `cargo about` can produce. **No platform could ever pass.** |
| A fresh checkout on a *Windows* runner re-expands every LF | The same file arrives ~9000 bytes **longer**, and `about.hbs` becomes CRLF too, so the template emits CRLF and the generated side moves ~1000 bytes as well |

So the two failures had mismatched sizes in opposite directions and looked unrelated. Fixed with
`.gitattributes` marking both files `-text`; the committed blob is now byte-identical to the
generator's output, 65 CRs and all, verified with `git hash-object -w --path`.

🔑 **The signature to recognise: byte counts differ, and the line diff is empty.** That is an EOL
problem and nothing else. It only became visible because `-Check` was made to print the diff —
before that the message was "out of date", which says nothing about which of these it is.

The check still runs on `windows-latest`, now for a much smaller reason: `notices.ps1` is what
regenerates the file and a maintainer always runs it on Windows, so that is where a failure is
reproducible. CI pins `cargo-about@0.9.1`, since an upstream reformat would otherwise fail looking
exactly like an unattributed dependency.

#### 🔴 Three separate traps produced the same "connection refused" page

All three make the webview point at `http://localhost:1420` when nothing is serving it.

1. **`cargo run -p griddle-app` is not enough on its own.** Without `custom-protocol` (below) it
   loads `devUrl`; building the frontend first changes nothing.
2. **`custom-protocol` was missing from `griddle-app`'s `Cargo.toml` entirely.** That feature is
   what makes a build serve `frontendDist` from inside the exe, and it is **not** one of
   `tauri`'s defaults — the stock template leaves it opt-in and lets `tauri build` add
   `--features custom-protocol`. So a hand-rolled `cargo build --release` produced a binary
   that started, titled its window correctly, and rendered nothing. It is now in `default`,
   which the template does not do: `tauri dev` passes `--no-default-features`, so hot reload
   still works while the obvious command becomes the correct one.
3. **`beforeDevCommand` used `bun run --cwd <relative>`.** bun documents `--cwd` as taking an
   **absolute** path; a relative one works from the repo root by luck and fails with `ENOENT`
   from the Tauri CLI's cwd. Use `--filter @griddle/desktop`, which resolves through the
   workspace from anywhere. Note `frontendDist` *is* relative to `tauri.conf.json` — the two
   keys use **different bases**, which is what made this look inconsistent.

`tauri.conf.json` is schema-validated and rejects unknown keys, so an `_comment` entry is a
hard error. Notes like these belong here.

#### 🔴 "A window opened" is not "the app works"

The release binary was reported working on the strength of the process staying alive, the
window title being right, and the PE subsystem reading 2. **All three were true while the page
showed a connection-refused error.** Checking the binary for embedded strings was no better:
the JS is brotli-compressed so it is not greppable, and `localhost:1420` appears in the
embedded config in *both* modes.

The decisive test is behavioural — **listen on `[::1]:1420` and see whether the app connects**:

```powershell
$l = New-Object System.Net.Sockets.TcpListener ([System.Net.IPAddress]::IPv6Loopback, 1420)
$l.Start(); Start-Process target\release\griddle-app.exe
# $l.Pending() true  -> still dev mode
# stays false        -> serving the embedded frontend
```

🟡 It must be `::1`, not `127.0.0.1`: **Vite binds IPv6 only**, and `localhost` resolves to
`::1` first on Windows — a `127.0.0.1` health check reports "nothing listening" while the page
serves fine. That is the exact mirror of the CDP rule, where Steam binds **v4** and we must use
`127.0.0.1` and never `localhost`.

**Verified 2026-07-30:** `bun run app` serves and renders; the release exe runs with **no dev
server**, does not reach for 1420, loads 2930 apps from `appinfo.vdf`, grants the asset scope
to exactly the account's `grid/`, and has PE subsystem **2 = `WINDOWS_GUI`** — no console
flash, the wart this project exists to remove.

### 🟢 File layout, after the readability pass of 2026-08-01

Two conventions were introduced, and both are worth knowing before adding a file.

**Tests live in a `<module>_tests.rs` sibling**, wired up with `#[cfg(test)] #[path = "..."] mod
tests;`. Twelve modules were 40–50% test code, which meant scrolling past several hundred lines of
fixtures to reach the thing being tested. `#[path]` rather than a directory keeps the tests beside
their subject instead of in a folder that exists to hold one file.

🔴 **This made twenty file writes visible to `check-boundaries.sh` that had been hidden inside
exempt modules** — every one a test fixture in a tempdir. They are annotated with `boundary-ok`
rather than exempted by a `*_tests.rs` pattern, because a filename pattern would be exactly the
"heuristic about layout" that the check's own header rejects.

**`fsutil` is a fourth entry on the write-boundary allowlist**, and deliberately not a fourth
writer: `grid::store`, `steam::shortcuts` and `settings` each had their own copy of the
temp-write-fsync-rename dance, which is three chances to lose the `fsync`. It knows nothing about
what it is writing, and each caller maps its `WriteError` into its own error type in one line.

**`#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]`** sits once at each crate
root, replacing 35 copies of the same per-module attribute. Verified to still deny `unwrap` in
shipping code by adding one and watching Clippy fail.

Files split: `commands.rs` (1352 → 9), `styles.css` (1046 → 11, **bundled output byte-identical**),
`focus.tsx` (661 → 4), `appinfo.rs` (1200 → 3), `AssetBrowser.tsx`, `components.tsx`,
`Settings.tsx`. Comments lost their emoji markers, milestone references and discovery narratives;
the hazards stayed.

### `griddle-core` module map

| Module | What it is |
|---|---|
| `fsutil` | The atomic write — temp file, `fsync`, rename — plus `sibling_with_suffix`. On the write-boundary allowlist; see above. |
| `appid` | `AppId` newtype. Signed in `shortcuts.vdf`, unsigned in filenames **and** in the CDP APIs. **Contains no CRC32 function, deliberately** — the folklore algorithm is disproven and the way to never regress is for it not to exist. |
| `vdf::binary` | Binary KV1. Read **and** write, byte-exact, including the extra trailing `0x08`. Validated against the live client in S9. |
| `vdf::text` | Text KV1, read-only. Skips scalar siblings among numbered keys (`contentstatsid`); case-insensitive lookup; handles escapes, comments, `[$WIN32]` conditionals. |
| `logo` | 5-anchor position maths. Mirrors `packages/shared/src/logo.ts`; **both test against one JSON fixture** so they cannot drift. |
| `grid::names` | Filename rules + `AssetType` with Steam's measured ordinals. `siblings()` is the delete-set that keeps exactly one file per asset. |
| `grid::store` | **The only artwork writer.** Sibling cleanup → temp → fsync → rename. Writes a default logo position when a logo has none. Clearing a logo takes its `.json`; clearing the header does not. |
| `steam::locate` | Registry cascade with the lowercase/forward-slash normalisation. `locate_with()` takes the override as a parameter so tests need no `unsafe` env mutation. |
| `steam::account` | `ActiveUser` → `loginusers.vdf` → sole `userdata/` dir. **Refuses to guess** between several accounts. |
| `steam::library` | `libraryfolders.vdf` + `appmanifest_*.acf`. One corrupt manifest never empties the library. |
| `steam::librarycache` | Steam's own default artwork, resolved through `appinfo.vdf`'s index rather than by filename. **Read-only in shipping code** — the only write is a test fixture into a tempdir, annotated `boundary-ok`; nothing here writes into Steam's cache. `safe_join` refuses a path that would escape the app directory, lexically rather than through `Path`, so the guard means one thing on every host. |
| `steam::localconfig` | The `apps` map in `localconfig.vdf` — the offline "all games" source, and where playtimes come from. Read-only; reuses `vdf::text`. |
| `steam::process` | ToolHelp process enumeration; `-shutdown` → poll → relaunch. **The only minter of `SteamStopped`.** Waits on *processes*, never on the registry pid. |
| `steam::shortcuts` | Read/edit/write `shortcuts.vdf`. Round-trip verified on **load**; write needs a `SteamStopped` token *and* re-checks it. Mutation surface is `set_icon` / `clear_icon` only. |
| `vdf::appinfo` | `appcache/appinfo.vdf` reader. **Not the same format as `vdf::binary`** — v29 keys are u32 string-table indices. Extracts only `common/{type,name,clienticon}`. |
| `steam::apptype` | `common/type` → "does this belong in the library list". Every unknown resolves toward **showing** the app. |
| `focusgrid` (TS, `@griddle/shared`) | Spatial `(section, row, col)` navigation maths, DOM-free so it is exhaustively unit-tested. Modelled on z13gui's `internal/focusgrid`. See below. |
| `input` | Reads the controller natively via gilrs on **XInput**, gated on window focus, emitting semantic actions. `input::repeat` is clock-free and holds the tests; the gilrs loop is `cfg(windows)` so the Linux CI leg still builds. |
| `sgdb::key` | `ApiKey`. Custom `Debug` prints a fingerprint; **no `Display`, no `Serialize`** — leaking it is a compile error. |
| `sgdb::model` | Response types, every field read off a real response. Only `id` and `url` are required. |
| `sgdb::query` | Endpoint + filter selection. `Dimensions` is a closed set, every value probed. |
| `sgdb::client` | **The only place the key is used.** Concurrency cap 3, backoff with jitter, content-type checked before parsing. |
| `settings` | `%APPDATA%\<AppName>\settings.json`, atomic. **Third and last writer.** A corrupt file is preserved, never overwritten. Filters are **one shared set**, not per asset type — see below. |
| `settings::dpapi` | `CryptProtectData` round-trip for the API key. Windows-only, with **no plaintext fallback**. |
| `cache` | `%LOCALAPPDATA%\<AppName>\cache`. JSON on a TTL, images forever. Entries are self-describing, so a collision or torn write is a **miss**. |
| `base64` | Shared by `settings` (DPAPI blob) and `cdp` (image payloads). `is_base64()` is the JS-injection guard. |
| `browser` | Opens a link in the default browser via `ShellExecuteW`. 🔴 **Allowlisted to https on `steamgriddb.com`** — handing an arbitrary string to the shell launches whatever handler is registered for it, and `file:///…exe` would run a program. A Tauri webview ignores `target="_blank"`, which is why this exists at all. |
| `cdp::sentinel` | The `.cef-enable-remote-debugging` flag. **Created at startup**, reported in Settings → Diagnostics — see below. Never truncates a file someone else wrote. |
| `cdp::target` | Finds `SharedJSContext` and **refuses anything that is not Steam** — port 8080 is a very common dev-server port. |
| `cdp::client` | Minimal CDP: `Runtime.evaluate` + `addScriptToEvaluateOnNewDocument`. A JS throw is a distinct error, not silent success. |
| `cdp::SteamJs` | `probe` / `apply_artwork` / `clear_artwork` / `clstamp` / `app_name`. The live-apply path. |
| ~~`cdp::modules`~~ | 🔵 **Deleted with deliverable B.** Held the eleven structural finders, the CLSTAMP diff and per-feature degradation. All eleven targeted React components only an injected UI needed; see the reliability section for why removing the fragile subsystem beat monitoring it. |

### `griddle-app` — the desktop shell

| Module | What it is |
|---|---|
| `error` | `UiError { kind, message, action }`. The **`kind` is what keeps "Steam is running" and "network timeout" distinguishable** across the boundary; `action` is what the user should actually do. |
| `state` | Loaded once at startup. **Nothing here may stop the window opening** — no Steam, no key and an unreadable `appinfo.vdf` are all ordinary first-run states. |
| `commands` | The `invoke` surface, one module per group: `status`, `apikey`, `prefs`, `library`, `search`, `apply`, `icon`, `logo`, `reset`, `diagnostics`, `steamctl`. Thin; every decision belongs to `griddle-core`. 🔴 Re-exported with globs, not by name — `#[tauri::command]` generates hidden `__cmd__*` siblings that a named re-export drops, and the failure reads as a missing command. |
| `fatal` | A `MessageBoxW` for a startup failure with no window to report it in. `eprintln!` under `windows_subsystem = "windows"` reaches nobody, so a missing WebView2 runtime made the app simply not appear. |

🔴 **`commands::icon` and `commands::logo` are the two that are not thin**, and they are the ones
to check first when this table looks out of date. Both walk a ladder of their own — see the icon
and logo sections below — and both were added after this table was first written, which is
exactly how the row above came to list eight modules while ten existed. `scripts/check-claims.sh`
now fails the build on that particular drift.

**The apply ladder lives in `commands::apply_asset`:** live first, file-write as the floor. The
result says which path ran, so the UI can say whether a restart is needed rather than leaving
the user staring at unchanged art. Falling back is *not* an error — it carries
`fell_back_because` and renders as a note.

The `asset:` protocol scope is granted **at runtime**, because both paths depend on the install
and the account id and so cannot be set in `tauri.conf.json`. Two directories:

| Directory | Recursive | Why |
|---|---|---|
| the account's `grid/` | **no** | custom artwork; flat by construction |
| `appcache/librarycache/` | **yes** | Steam's default art. 279 of 2249 apps store theirs one level down under a sha1 directory, and a non-recursive grant would 403 exactly those |

🔴 **The recursive flag is the trap.** Copying the `grid/` grant's `false` looks right and fails
only for the nested minority — which reads as "some games have no art", indistinguishable from
the cache genuinely not having it. It is not unit-testable; verify by launching the app and
confirming **1030300** renders. 620 rendering proves nothing, because it is flat.

This is a real widening — ~2249 directories of Steam-owned store artwork — accepted because it
is still far narrower than the Steam root and contains nothing but public images. A grant
failure is a `warn!`, not fatal: the UI then falls through to the CDN and still shows art.

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

> 🟢 **Re-probed in full on 2026-08-02, and every claim below held.** This section was the one
> class of the 2026-08-02 audit that could not be checked at the time, for want of a key, and it
> was reported as an outstanding gap rather than quietly assumed. It is now closed.
>
> Portal 2 resolved to `#17830`, an unknown appid came back as `None` rather than an error, all
> five asset endpoints answered, every dimension value was accepted by its own endpoint, page 1
> differed from page 0, and a CDN thumbnail downloaded with a valid JPEG magic. Counts move with
> the site and are not findings: 423 assets across the types on 620 this run.
>
> 🔑 Two things worth noting because they are *negative* results that still hold. `logos` and
> `icons` return **no** dimension vocabulary at all — the same fact recorded below as "icons
> rejects `dimensions` outright". And `heroes?dimensions=1600x650` is still **valid with 0
> matches**, so an empty result there remains not-a-bug.

Reproduce with `$env:SGDB_API_KEY = "<key>"; cargo run -p griddle-core --example sgdb_probe`
(read-only). The key is read from the environment and **never** from a file in this repo.

⚠️ **Set it in the process, not in a file.** The probe above was run by assigning `$env:SGDB_API_KEY`
for one invocation and clearing it afterwards. A `.env` is gitignored, but `check-secrets.sh` only
scans what git tracks — so a key in an ignored file is invisible to the guard rather than blocked
by it, and lives on disk until someone remembers to delete it.

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

🔴 **Plenty of Steam appids are not on SteamGridDB, so appid alone is not a resolver.**
`[VERIFIED-BOX 2026-07-30]` `/games/steam/3837340` (FINAL FANTASY VII, a re-release) **404s**,
while `/search/autocomplete/FINAL FANTASY VII` returns the game as its first hit. Every non-Steam
shortcut 404s by construction — its appid is a random high-bit number Steam generated locally and
SteamGridDB has never seen it. Note the neighbours all resolve: `1004640`, `2909400`, `1173800`
are all 200, so this is not "the whole series is missing", it is per-appid and unguessable.

`commands::resolve_game` is therefore a three-rung ladder — manual override, then appid, then a
name search — cached per session **in memory, not in `settings.json`**: a name match is a guess,
and persisting it would enshrine a wrong one somewhere the user has to notice and undo it.

🟢 **`GET /games/id/{id}` exists and returns a full record.** `[VERIFIED-BOX 2026-07-30]` Probed
because a manual game override stores only SteamGridDB's id, and an override written before the
name was stored alongside it would otherwise display as `SteamGridDB game #17830`. Current
overrides need no request — the name is captured when the user picks it — so this is a
fallback for old entries only.

🟢 **The site's asset pages are `/{grid,hero,logo,icon}/<id>`, and asset ids are numbered _per
type_.** `[VERIFIED-BOX 2026-08-01]` `grid/1`, `logo/1`, `icon/1` and `hero/100` all return 200
with a title naming the game and the author; `hero/1`, `grid/99999999` and `nonsense/1` all 404.

🔴 The three `…/1` pages are **three different artworks by two different authors**, which is what
settles the id question: an id is unique within its type and means nothing without one. Anything
keyed on a bare asset id — an open modal, a selection, a cache entry — collides the moment the
asset type changes underneath it. `assetPageUrl` also collapses both capsule types to `/grid/`,
mirroring the API, where `grids` serves both.

⚠️ **Probing this needs a browser `User-Agent`.** The *site* is Cloudflare-gated and 403s a bare
client — the opposite of the API, per S11 — so `WebFetch` and a default client both report 403 for
every path, valid or not. Read as "the URL is wrong", that 403 is exactly backwards.

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

#### 🟢 The module finders, and how each ambiguity was resolved

`cargo run -p griddle-core --example cdp_disambiguate -- <FinderName> --token <STR>` prints every
candidate module with its size, an excerpt around the anchor, and whether each probe token is
present. **Ambiguity is never resolved by taking the first match** — that freezes a coin-flip
into the settings file, where it then looks resolved forever. `Outcome::Ambiguous` is a
distinct, unusable state for exactly that reason.

Four predicates were too loose on first contact with the real bundle:

| Finder | Also matched | Discriminator | Why |
|---|---|---|---|
| `FocusableFactory` | 4690, 60291 | `gamepadEvents` | 4690 is the focus *tree node*; 60291 is a dropdown that merely uses `preferredFocus`. |
| `SteamArtworkFlow` | 80818, 81659 | `CloseModal` | Both do the same base64 strip — see below. |
| `AssetTypeNames` | 19807 | `SetCustomArtworkForApp` | 19807 is webpack's **asset manifest**, listing paths like `./google_chrome/library_capsule.png`. |
| `ModalHost` | 91435 | `showModal` | 91435 only *passes* those props along; the host is what calls `<dialog>.showModal()`. |

🔴 **Correction: the hardcoded `"png"` is one call site's choice, not a universal rule.** Module
87498 really does call `SetCustomArtworkForApp(e,r,"png",t)` with the literal — but 80818 and
81659 perform the identical `slice(indexOf("base64,")+7)` strip and pass a **variable** mime.
The earlier note implied Valve always hardcodes it. What is actually verified is narrower, and
still sufficient: 87498 hardcodes it, and S4 proved animated WebP labelled `"png"` animates in
both the desktop library and Big Picture, because Chromium sniffs content.

**Live apply needs no finders at all** — it calls `SteamClient.Apps.SetCustomArtworkForApp`,
which the CEF host binds and Valve cannot rename without breaking their own client. The most
valuable feature is the least exposed to a Steam update. Worth keeping true.

### 🟢 Controller navigation — the focus model

Three layers, split so the model is testable without a controller and the input source can change
without touching it: `packages/shared/src/focusgrid.ts` (pure maths) → `apps/desktop/src/focus.tsx`
(DOM: measurement, real focus, overlay stack) → the views, which register their controls.

**Sections stack vertically and are ordered by document position**, not by a hand-maintained index.
Within a section, items sit at explicit row/column indices — except wrapping grids, which register
by *index* and have their row/column derived from a **measured** column count.

🔴 **The column count cannot come from React state.** `repeat(auto-fill, minmax(var(--tile), 1fr))`
resolves against the window, so `.library`, `.assets` and `.slots` are all measured by grouping
children on `offsetTop`. `.assets` is the trap: its tile width changes **per asset tab** (9.5rem
capsules → 22rem heroes) *without the container resizing*, so a `ResizeObserver` alone silently
keeps the previous tab's count.

🔴 **The tile-size control made that worse, and the existing mitigation did not cover it.**
`useFocusGrid` also watched `childList`, which catches a tab change only because the *children*
change with it. Stepping the zoom re-flows **the same children in a container of the same width**
— neither observer fires. The fix is `attributes: true, attributeFilter: ['style', 'class']`,
since the control's whole mechanism is writing `--tile` to the container's inline style.

A stale count is invisible in the worst way: every tile renders correctly and only *navigation* is
wrong, so pressing down moves two rows and it reads as the focus model being broken rather than as
a measurement nobody retook. Pinned by a test in `focus.test.tsx`, confirmed to fail with
`attributes` removed.

#### 🟢 The logo positioner, and two things it did *not* need

The geometry was already written — `logo.rs` and `logo.ts` against one shared fixture — for the
Big Picture UI that was cut, and none of it needed touching. What was missing was everything
around it: a CDP call, a command pair, and a preview.

🔴 **`SetCustomLogoPositionForApp` takes a JSON _string_, not an object.** Valve's own call site
is `SetCustomLogoPositionForApp(e.appid, JSON.stringify({nVersion:1, logoPosition:t}))`. Passing
the object is the obvious mistake and Steam ignores it silently. The payload is built with
`serde_json` from `logo::LogoPosition` — the same type and the same `serde` renames the file
writer uses, so `pinnedPosition` cannot acquire a second spelling — and then serialised *again*
to produce an escaped JS string literal.

🔵 **The file is written on the live path too**, unlike an artwork apply. S3 *measured* Steam
writing the artwork file itself; whether it does the same for a logo position has never been
checked, so rather than assume it, `set_logo_placement` writes unconditionally. It costs one small
JSON document and guarantees a later read agrees with what was applied.

⚠️ **`resizeByDpad` and `rampStep` are still unused.** They implement arrow-key resizing, which
cannot work in the desktop app: arrows belong to the focus model, and a control that swallowed
them would trap the cursor with no way out. The positioner steps with buttons instead. Left in
place rather than deleted because they are covered by the shared fixture and cost nothing; if that
stops being true, delete them rather than inventing a caller.

🔵 **The tile-size control took three placements to land, and the last one was the maintainer's
call.** Worth recording because the first two each fixed a real problem and each looked wrong:

1. **A plain row above the results.** Visible on a fresh tab, gone after one screenful of infinite
   scroll — reported as *"I don't see the zoom controls."*
2. **In the sticky toolbar**, which fixed reachability and created a symmetry problem: that row
   already carries a scope switcher, a filter box, a sort group and a count, so a fourth group had
   nowhere to sit that did not read as an afterthought. Restyling it to match the others did not
   help; the issue was the crowding, not the buttons.
3. **The right-hand half of the nav row**, opposite the Library/Settings tabs. `.tabs` has been
   `justify-content: space-between` with a single child since it was written — the slot was
   designed and never filled.

⚠️ **The nav row is not sticky**, so this trades reachability back for symmetry: scroll far enough
and the control leaves with the header. That is a deliberate choice, not an oversight — the
library's scope/filter/sort bar below it stays pinned either way.

**It is a portal, not lifted state.** The zoom *target* depends on which asset tab is open, which
only `AssetBrowser` knows; threading that up through `App` to render one control would invert the
ownership. `navSlot.ts` holds the anchor in context and the views `createPortal` into it. React
portals render at the anchor's real DOM position, so the focus model still sees the buttons in the
nav row — columns 2 and 3 of the `nav` section, after the two tabs.

🔵 **No numeric readout, after briefly having one.** It showed a percentage of the target's own
min–max window, so an ordinary size read *"25%"* and invited the question of what 100% would be —
a number with no meaning outside the control. The grid resizing under the press is the feedback.

🔵 **`ZoomTarget` is wider than `AssetType`.** The library list and the Current overview are not
asset types but they are the same thing to a user — a wrapping grid of pictures they scroll — and
keying zoom by asset type alone left the grid people scroll *most* as the one that could not be
resized. Rust validates the seven names in `set_zoom`; `parse_asset_type` still accepts only the
five, with a test pinning that the two extras are refused by it.

🔵 **`ZOOM` in `@griddle/shared` is now the only copy of the tile widths.** `assets.css` had
per-type rules (15rem wide capsules, 22rem heroes, 13rem logos) *and* the table declared its own
numbers in different units — hero and logo were measured in whole **columns**, a second mechanism
the stylesheet had never implemented, because nothing had ever consumed the table. CSS now reads
`--tile` and keeps one fallback for first paint. Stored under the **wire** names (`grid_p`), like
every other value crossing the boundary; the range is clamped on *read*, so a value saved under
one build's bounds survives a later one narrowing them.

**Enter and Space are deliberately not intercepted.** Focus is real DOM focus, so the browser
already fires `click` on a focused `<button>` for both; handling them too would apply every piece
of artwork twice.

#### 🟢 The controller is read natively, and the backend choice was measured

`griddle_core::input` polls the pad on its own thread and emits **semantic actions** — the same
`up/down/left/right/accept/back/menu` vocabulary the keyboard produces — over a Tauri `nav` event.
So a pad is a second *source* for navigation, not a second implementation of it.

🔴 **Not `navigator.getGamepads()`.** Two open WebView2 bugs rule the web API out, and
[#5507](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5507) is the one that matters:
**gamepad input dies in WebView2 apps whenever the Steam Overlay is attached** — which is always
true of Griddle launched from Big Picture, the entire point of the feature.
[#3025](https://github.com/MicrosoftEdge/WebView2Feedback/issues/3025) is a second, independent
one. Reading natively also works *with* Steam: the overlay hooks XInput/DirectInput/RawInput/WGI
and injects an emulated Xbox pad, so Steam Input mappings arrive for free. `[VERIFIED-SOURCE]`

🔴 **gilrs defaults to Windows.Gaming.Input, which needs an in-focus window — so `pad_probe`
enumerated zero controllers on a machine whose PnP tree listed an "XINPUT compatible HID device".**
`[VERIFIED-BOX 2026-07-31]` Switching to `default-features = false, features = ["xinput"]` found
`Xbox Controller` immediately. The app *has* a window so WGI would have worked there, which is the
trap: the backend would have looked fine in the product while the diagnostic harness built to
debug it was structurally blind. XInput has no window requirement and is what Steam Input targets.

🔴 **`connected()` returns a `Vec<String>` rather than logging.** "The pad is not being read" and
"the UI is ignoring the pad" look identical from outside and share no causes — and a `warn!` is
invisible in a `windows_subsystem = "windows"` binary with no console, so the question has to be
answerable on demand. `cargo run -p griddle-core --example pad_probe` prints the pad list first,
then every action live.

#### 🔴 `rename_all = "lowercase"` is a trap that springs on the *second* word

`[VERIFIED-BOX 2026-08-01]` LB and RB did nothing while A, B, Y and every direction worked.
`Action` carried `#[serde(rename_all = "lowercase")]`, so `TabPrev` crossed the boundary as
**`"tabprev"`** while `NavAction` in `focus.tsx` matched `'tabPrev'`. Every previous action was a
single word, for which `lowercase` and `camelCase` are identical — so the attribute had been
harmless for its whole life and became wrong the moment the vocabulary grew a two-word entry.

Nothing catches this. There is no type shared across the boundary, no runtime error, and the event
is delivered successfully — it simply matches no branch. `input::wire_tests` now pins every string,
and was confirmed to fail by reintroducing `lowercase` before being trusted.

**Two wrong diagnoses came first, both from reading instead of measuring:**

1. *"gilrs' SDL mapping must disagree for the bumpers."* `describe_buttons()` was written to check
   and **exonerated the mapping** — LB resolved to `EvCode(18)` mapped *and* native, exactly like
   A's `EvCode(12)`. That negative result is what redirected the search.
2. *"then `is_pressed` must be the problem"* — the edge buttons were rewritten to read gilrs'
   `ButtonPressed` events instead. Better code, kept, **and not the bug.**

What settled it was `examples/pad_probe` logging raw gilrs events beside the actions derived from
them: `ButtonPressed(RightTrigger, EvCode(19))` → `TabNext`, emitted perfectly. Once the Rust side
was visibly correct, the only place left was the wire. **Two rounds of plausible reasoning lost to
one round of looking**, which is the same lesson as the CRC32 folklore and the CDN-burst theory.

#### 🔴 The controller did nothing, and nothing anywhere said why

`[VERIFIED-BOX 2026-08-01]` Everything on the Rust side was correct — the window label resolved,
`connected()` reported `["Xbox Controller"]`, `emit` returned `Ok(())` — and the pad still moved
nothing. **`listen` is a core plugin command, so Tauri v2's capability system gates it**, and this
app had *no capability file at all*: `gen/schemas/capabilities.json` was `{}`. Every subscription
was refused with *"event.listen not allowed"*.

It was invisible for a second, compounding reason: `listen()` returns a promise, the code kept it
only to unsubscribe with, and **never attached a `catch`** — so the refusal was an unhandled
rejection that surfaced nowhere. `Ok(())` from `emit` means Tauri accepted the event for delivery,
not that anything received it.

Why our own commands kept working throughout, which is what made this so confusing: **`#[tauri::command]`s
defined by the app are not capability-gated.** Only plugin commands are, and `core:event` is a
plugin. So the library loaded, settings saved and artwork applied while the one core-plugin call in
the product was denied.

`capabilities/default.json` now grants exactly `core:event:allow-listen` and
`core:event:allow-unlisten` — not the `core:default` bundle, which would hand the webview a pile of
commands the UI never calls.

**How it was found, after two wrong diagnostics:** `document.title` was tried first and proved
nothing, because a Tauri window's native title does not follow the document's. What worked was
making the frontend *render* what it saw — a temporary fixed-position badge, screenshotted — which
printed the refusal verbatim in one run. **When a signal crosses a process boundary, put the probe
on the far side of it.**

**`FocusGate` is load-bearing, not a nicety.** The thread reads the pad *globally*; nothing scopes
it to our window. Without gating on window focus, playing a game with a controller would also be
driving Griddle in the background. Same lesson as z13gui gating every event on visibility. The
repeater is still fed `None` while the gate is shut, so a direction held across an alt-tab does not
fire a stale step when focus returns.

`input::repeat` is clock-free — `advance(held, now_ms)` — so the awkward parts are **nine unit
tests** rather than things to discover with a controller in hand: a late poll must not bank up a
burst, releasing must reset the ramp, changing direction mid-hold must take ownership immediately,
and a perfect 45° diagonal must resolve to exactly one direction *deterministically* or a stick
held on the diagonal emits right, up, right, up.

#### The three bugs this found in existing code

**🔴 The first directional press inside an overlay was silently swallowed — and it *looked* like
the model was broken.** Opening a scope cleared the selection so the first press would land inside
the overlay rather than resuming behind it. But `autoFocus` had already drawn a `:focus-visible`
ring on Cancel, so the user saw a selection, pressed right, and watched the ring stay put. Found by
driving the reset dialog from the keyboard and comparing two screenshots that should have differed
and did not. A scope now takes a selection immediately, preferring whatever it chose to `autoFocus`
— those choices are deliberate (`GameSearchModal` wants the caret in its search box, `ConfirmReset`
wants Cancel, so the destructive button is never the default).

**🔴 `disabled` on every tile during an apply is invisible with a mouse and fatal to a pad.**
`AssetBrowser` disabled all ~50 tiles whenever any apply was in flight. A disabled button cannot
hold focus, so the grid emptied out mid-action and focus was flung elsewhere. Only the tile being
applied is disabled now, with `aria-busy` on the container carrying the meaning — **and the
re-entry guard had to be made explicit**, because it had been an accidental side effect of the
tiles being unclickable.

**🔴 Two uncoordinated `window` Escape listeners, and two overlays with none.** `ContextMenu` and
`ArtPreview` each hand-rolled one, both unconditional, so one press dismissed both when they
overlapped; `ConfirmReset` and `GameSearchModal` could not be dismissed by keyboard at all. All
four now go through the scope stack, which also restores focus to whatever opened them — there was
**no focus restoration anywhere in the app** before this.

⚠️ **`styles.css` had zero `:focus` rules across 998 lines.** With a controller the ring *is* the
cursor, so it ships as `outline` with an offset — not a border, which would resize the element and
reflow the grid, and not an inset ring, which `overflow: hidden` on `.asset-button`/`.slot-art`
would clip.

⚠️ **Automated keyboard testing does not work here.** `SendKeys` reaches the Tauri window but the
WebView2 child does not take keyboard focus from `SetForegroundWindow` or `AppActivate`, so the
page never sees the keydown. Screenshot-driven runs are reliable *only* immediately after
`Start-Process`, while the window still holds genuine focus. The keyboard pass is a manual step.

#### Twenty-one bugs worth remembering

**🔴 A dismiss-on-click listener in the *capture* phase eats the menu's own clicks.** The
right-click menu closed on `window.addEventListener('click', close, true)`. Capture runs before
the event reaches the menu item, so React unmounted the item mid-dispatch and its `onClick` never
fired: every menu action silently did nothing. The menu looked perfect, which is what made it
convincing — *"the UI renders nicely, but the revert function doesn't work."*

The dismiss listener must ignore clicks **inside** the menu (`menu.contains(e.target)`) and let
the item's own handler close it. Capture is still right for everything outside, so the menu
closes even when something below stops propagation.

**How it was found, because guessing was getting nowhere:** the grid directory was listed before
and after the user tried a reset — **no file was deleted** — and `scan` was extended to print
what `GridDir::existing` sees, which was every file including the shortcut's `.ico`. Backend
correct, no error shown, nothing deleted ⇒ the click never arrived. Two read-only observations
beat four rounds of plausible theorising.

**🔴 Animated artwork's *thumbnail* is a `.webm` video, and an `<img>` renders it as a broken
image.** SteamGridDB serves the full asset as WebP or APNG but the preview as a video. Dropping
that into `<img src>` produces a broken-image icon — which is indistinguishable from missing
artwork, and got reported as "this game is missing a lot of entries". **23 of 200** Cyberpunk
2077 capsules (12%) were affected. `[VERIFIED-BOX 2026-07-30]`

🔴 **The obvious fix is wrong: do not key off the mime.** `mime === "image/webp"` misses a third
of them, because an APNG is animated too and also gets a `.webm` preview. Measured cross-tab:

| thumb | mime | count |
|---|---|---|
| `.jpg` | `image/png` | 139 |
| `.jpg` | `image/jpeg` | 27 |
| `.webm` | `image/webp` | 16 |
| `.png` | `image/png` | 11 |
| `.webm` | **`image/png`** | **7** |

The predicate is the **thumbnail's extension** (`isVideoPreview`), and it reads the path so a
query string can neither defeat nor fake it. Note the CSP already carried
`media-src https://cdn2.steamgriddb.com` — the policy anticipated this and the renderer never
caught up, which is its own lesson: a permission granted is not a feature implemented.

**🔴 An `IntersectionObserver` fires on *change*, so a callback ignored once is a callback that
may never come again.** The infinite-scroll observer starts observing during the page-0 fetch;
its initial callback lands while that request is in flight, hits the in-flight guard, and does
nothing. `setPage(0)` on page 0 changes nothing, so the effect did not re-run to try again — and
from then on only the sentinel physically moving in or out of view could wake it. If the page
that arrived was too short to push the sentinel past the 400px margin, the browser sat there
showing a fraction of the results, with no error and no spinner. The symptom was "games with a
lot of artwork are missing entries", which sounds like an API problem and is not.

The fix is to stop waiting for a *change*: re-observe after every settled load (`assets.length`
and `loading` in the dependencies) so there is always a fresh initial callback. Plus a visible
**Load more** button, because infinite scroll depends on viewport geometry working out and when
it does not there is otherwise nothing the user can do. The count now reads *"12 of 400"* rather
than *"400"* — a total-only count is what let this stay invisible.

🔴 **The `[INFERRED]` explanation attached to this was wrong, and probing killed it.** The guess
was that SteamGridDB paginates before filtering, returning a short page 0. Measured against the
live API: Cyberpunk 2077's filtered capsule query returns `total=728` with a **full 50 items on
every page**, pages 0, 1 and 2 alike. There is no short page. The stall is real and proven from
the code, but the thing that actually made this game look broken was the `.webm` thumbnails
below. **An inference sitting next to a proven fact borrows its credibility — probe it or drop
it.**

**🔴 The shared filter set is clamped at *query* time, never on save.** `pruneToType` narrows the
one shared `Filters` to what the current tab's endpoint accepts, and the result is deliberately
thrown away after building the query. Pruning on save would be the obvious simplification and is
a data-loss bug: the Logo tab offers no sizes at all, so opening it once would wipe every size
the user had chosen, for every tab, permanently. Tests assert both halves — that a query is
clamped, and that a round trip through storage is not.

**🔴 A settings migration that would have failed *quietly*.** Filters moved from
`{"grid_p": {…}, "hero": {…}}` to one flat object. Serde reads that old map into the new
`FilterState` as "every field missing" → all-`false`, which is **not** `defaultFilters()` — it
reads as though the user had deliberately switched every content filter off. No error, no panic,
just wrong. `filters_compat` recognises the old shape (its values are all objects; a current
`FilterState` holds booleans and arrays) and carries `grid_p` across. The same applies to
`game_overrides`, which went from a bare id to `{id, name}`. **A type change under `#[serde(default)]`
is not a no-op — it is a silent default.**

**🔴 One piece of state that belonged to a *variant*, refilled asynchronously — and the wrong
answer was plausible, not an error.** The asset browser held a single `Filters` value and
repopulated it from `prefs()` on every tab change. That left a window in which the state belonged
to the *previous* tab, so switching Capsule → Wide Capsule issued a request carrying `600x900`.
Both tabs are the **same** `grids` endpoint separated only by `dimensions`, so it returned real
artwork — portrait capsules, in the wide tab. It looked like the tab switch was being ignored.

`query.rs` already carries a warning about exactly this failure ("fills it with portrait art…
which is worse than failing") and the code still walked into it *from a different direction*: the
warning was about sending **no** dimensions, and this sent the **wrong** ones. Fixed by making the
invariant unrepresentable — `filtersForType(type, edited, stored)` resolves synchronously from a
per-type map and cannot return another tab's filters. **A documented hazard is not a solved one
unless the shape of the code enforces it.**

**🔴 A guard that was right for one caller, applied to every caller, turned a flicker into a
permanent wrong state.** `if (inFlight.current) return;` exists to stop the infinite-scroll
observer requesting the same page twice. But the *reset* path used the same function, so when a
tab or filter change fired while a request was in flight, the corrective fetch was refused — and
never retried. The stale response then landed and stuck. That is what turned the bug above from a
brief flash into "this tab is broken". Now every request carries a generation and a newer one
supersedes an older one; the in-flight guard applies only to the observer, which is the caller
that actually needs it.

**🔴 Reading a setting that a separate round trip had not written yet.** The library list read
`library_sort` from `Settings` while the UI persisted the choice through a *different* command.
Picking "Recently played" reloaded the list before the write landed, so it came back in the old
order and the control looked dead. `scope` and `sort` are now **parameters** to `library`. A value
the caller already has should be passed, not re-read from shared state.

**🔴 `<details open={…}>` is controlled by React.** Binding `open` to "are the filters modified"
slammed the panel shut the instant the user clicked "Reset filters", with their cursor still
inside it. Seed from the derived value once, then let the user own it.

**🔴 A path-traversal guard built on `Path::components()` means different things on different
hosts.** `librarycache::safe_join` walked the components and refused anything that was not
`Normal`, which is correct on Windows — `C:\windows\system32\evil.jpg` is a `Prefix` and is
refused. On Linux a backslash is an ordinary filename character, so the whole string is **one
`Normal` component** and the guard accepted it, joining it under the app directory.

Harmless in the shipped product, which is Windows-only, and that is exactly why it is worth
recording: the test asserting the refusal **passed on every machine anyone ran it on**, and only
the Linux CI leg — whose stated job is catching platform-dependent behaviour — could see it. The
rejection is now lexical (`contains('\\')`, plus a `<letter>:` drive-relative check) *before*
`Path` is consulted, so it means one thing everywhere. Nothing legitimate is lost: Steam writes
`name.jpg` or `<sha1>/name.jpg`, always forward-slashed.

**`Path::ends_with` matches whole components, not string suffixes.** `p.ends_with("_icon.ico")`
is always false for `4048848997_icon.ico`. Compare `file_name()` instead.

**`StateFlags` is a bitfield, not an enum.** `6` = `StateFullyInstalled | StateUpdateRequired` —
installed *and* update-pending, which is playable; FINAL FANTASY TACTICS reads `6` here. A test
asserting `6` meant "not installed" failed against correct code.

**🔴 The gate did not run `tsc`, so a broken typecheck stayed green locally for three
milestones.** `tsconfig.json` set `types: ["bun-types"]` but the package was never a dependency,
so `bun run typecheck` — which **CI runs** — failed with `TS2688` while every local gate passed.
That is exactly the drift the gate exists to prevent, and the fix is the gate running the same
checks CI does, not a note to remember. `tsc typecheck` is now a gate step.

**🔴 Removing a filter value because it 400'd on the wrong endpoint.** `512x512` and
`1024x1024` were dropped from `Dimensions` after they failed against `icons` — but they are
**valid for `grids`** (9 and 22 assets for Portal 2). The real rule is that dimension values
are *endpoint-specific*: `heroes?dimensions=600x900` is also a 400. Both are back, each variant
now knows its endpoint, and `AssetQuery::validate_for` refuses a mismatch locally rather than
letting it surface as "SteamGridDB rejected the request". **A value that fails one endpoint has
been disproved for that endpoint, not in general.**

**🔴 A webpack chunk id of `{}` stringifies to `"[object Object]"`, so only the _first_ module
scan of a Steam session worked.** webpack keys installed chunks by id; the second push found
the chunk already installed and never invoked the callback, so `__webpack_require__` was never
handed over. The scan failed with "the module registry was not handed over" — and had it
returned *empty hits* instead of a distinct error, it would have looked exactly like Steam
removing every component at once, which is the worst possible false alarm for this design. Now
uses a fresh `Math.random()` marker per call, with a regression test on the generated script.
The spike's snippet did use `Math.random()`; the *reason* was never recorded, so it got dropped
in the rewrite — which is precisely what "record the predicate, not just the conclusion" is for.

**🔴 A `\` at the end of a line in a Rust *raw* string is literal, and three tests passed
anyway.** The CDP fixtures wrapped a long user-agent across lines inside `r#"..."#`, producing
invalid JSON. The tests that expected a *rejection* all still passed — for the wrong reason (a
parse failure, not the identity check they claimed to test). Only the **control** test, the one
asserting a real Steam handshake *succeeds*, could catch it. Same lesson as the focus-tree
probes: an all-negative test suite cannot tell "correctly refused" from "broken fixture".

**🔴 A hand-rolled FNV-1a had a mistyped prime, and the empty-string vector still passed.**
`0x1000_0000_01b3` is one hex digit too many; the correct prime is `0x100_0000_01b3`. The
`""` case cannot catch it — that result is just the offset basis, returned before any multiply
happens. `"a"` is the vector that matters, and it is also what distinguishes FNV-1**a**
(`…dc4c8601ec8c`) from FNV-1 (`…bd4c8601b7be`), which differ only in whether the xor precedes
the multiply. A second mistake surfaced immediately after: the `"foobar"` constant belonged to
neither variant. **Published vectors for a hand-rolled primitive, and pick ones that exercise
the loop.**

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

Reproduce with `cargo run -p griddle-core --example cdp_probe` (add `--probe2` for the
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
tree that owns the focus contexts. `[INFERRED — ARCHIVED, never verified]`

🔵 That `VERIFY` was outstanding when M6 was cut, and it stays outstanding: the deliverable it
belonged to no longer exists, so there is nothing to verify it *for*. Left in place, relabelled,
because reviving M6 would start here — but it must not be read as a fact. It never was one.

---

### 🟢 S2 PASSES — the Big Picture deliverable is viable

🔵 **Kept after the deliverable was cut.** This proved injection *possible*, and the finding stands
— it just is not being used. It is preserved in full, wrong turns included, so that reviving M6
later means re-reading rather than re-deriving. The code is in git history; only this survives in
the tree.

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

- **Non-Steam shortcuts** — write `<appid>_icon.<ext>` into `grid/` **and** repoint the
  shortcut's `icon` field. 🟢 **That does not require closing Steam**, see below.
- **Real Steam apps** — the ordinary file write into `grid/<appid>_icon.<ext>`, shown after a
  Steam restart. No `shortcuts.vdf` step, because a Steam app has no entry in it.

#### 🟢 `SteamClient.Apps.SetShortcutIcon` exists, and it removes the whole shutdown dance

`[VERIFIED-BOX @ CLSTAMP 10856968, 2026-08-01]` `SteamClient.Apps` has **120** members, twelve of
them `SetShortcut*` — including `SetShortcutIcon`, beside `SetShortcutExe`, `SetShortcutName` and
`SetShortcutStartDir`.

Called as `(appid, path)` with a shortcut's **existing** icon value — a no-op assignment, so
nothing was changed to find out — it returned without throwing. That fixes the argument order;
arity cannot be read off a native binding, whose `.length` is always 0.

🔴 **This is why the Decky plugin never restarts Steam, and the earlier reasoning here missed
it.** The problem was framed as "`shortcuts.vdf` cannot be written while Steam runs", which is
true, and then solved by closing Steam — a shutdown/relaunch flow behind a confirmation dialog.
The plugin runs *inside* Steam and simply asks Steam to make the change. So does this app now:
`cdp::SteamJs::set_shortcut_icon` with Steam up, direct file edit with Steam down.

The lesson is narrow and worth keeping: **the file was never the only interface.** Every other
part of this product already prefers asking Steam over editing its files, and the icon path was
built the other way round because S8's finding — that the *artwork* API cannot set icons — was
generalised into "no API can". One `Object.keys` would have said otherwise.

⚠️ The icon still does not appear until Steam restarts, whichever route applied it. That is not a
defect to design around; say it in the toast, as the plugin does.

#### 🔴 This section told the Icon tab to disable itself for Steam apps. That was wrong, and it shipped as a regression before being caught

It read: *"writing one still has no route, so it stays disabled for Steam apps with an
explanation… rather than a control that silently does nothing."* Acting on it replaced the
working Icon tab with a paragraph explaining that Steam games cannot have custom icons.

**They can.** The maintainer reported it plainly: the Decky plugin replaces icons for Steam
games perfectly well, it just cannot do it without a restart — and **Griddle already did it too**,
through the same file-write floor every other slot falls back to. Nothing was measured to support
the claim; it was inferred from decky writing into `appcache/librarycache/`, and the inference
does not follow. That decky uses one route is not evidence that no other route exists.

Two lessons, and the second is the expensive one:

1. **"No route" was never `[VERIFIED-BOX]`.** S8 measured that the *live API* is a no-op for
   icons. It measured nothing about the file path, which is what actually applies them.
2. 🔑 **A feature that already works is evidence.** Removing working behaviour on the strength of
   a document — over the observable fact that the app was doing the thing — is backwards. When
   this file and the running product disagree, the product wins and this file gets corrected.

The needing-a-restart part is not a defect to design around; it is simply what an icon costs.
Say so in the toast and move on.

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

`[VERIFIED-BOX 2026-07-27]` Reproduce with `cargo run -p griddle-core --example set_shortcut_icon`
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
document the click-through.

#### 🔵 The signing landscape moved, and both halves of what this said are now wrong

This used to end *"Signing is a v1.1 problem — Trusted Signing wants a 3-year-old legal entity."*
Checked 2026-08-02, and that requirement is **gone**: Microsoft dropped it when the service left
public preview, and self-employed individuals can now sign up. It has also been renamed **Azure
Artifact Signing**.

🔴 **The second wrong half is the one that matters more, because it is the assumption the whole
"get a certificate" plan rests on: signing no longer silences SmartScreen.** Microsoft's own
comparison table, revised 2026-04-21, gives *"Reputation builds over time; initial warnings
expected"* for Artifact Signing **and** for OV **and** for EV, and states plainly that **EV's
instant bypass was removed in 2024**. Every tutorial still recommending EV for this is describing
a behaviour that no longer exists. The only row in that table with "no warnings" is publishing an
**MSIX through the Microsoft Store**, where Microsoft re-signs the package.

⚠️ **Two sources disagree, and this is recorded rather than resolved.** Microsoft says reputation
builds over time; a practitioner blog reports *"instant reputation on all executables signed"* and
no SmartScreen popups on first release. Both are `[VERIFIED-DOCS]` at best — the weakest tag, and
this file's own header notes it had been used zero times, which was the right number. **Do not
spend money on the strength of either.** The cheap resolution is empirical: sign one release and
download it on a clean machine, which is the same test M8 already owes for the unsigned case.

What is worth knowing regardless of who is right: **reputation accrues per certificate, not per
file.** Unsigned, every release starts from zero because the hash changes. Signed with a stable
certificate, reputation carries across releases. That argument holds under either account of the
first-download behaviour, and it is the real reason to sign.

🔑 **For this project specifically, the cheapest path is free: SignPath Foundation.** It gives
qualifying open-source projects OV-level signing through a managed HSM pipeline at no cost, and
Griddle fits the criteria as stated — public repository, OSI licence (Apache-2.0), CI-built.
Microsoft's own page links to it. Artifact Signing at ~$9.99/month is the fallback, with one new
catch to check first: **individuals are limited to the USA and Canada** (organisations get the
USA, Canada, EU and UK).

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

`[VERIFIED-BOX @ CLSTAMP 10840511, 2026-07-27]` Run `cargo run -p griddle-core --example
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

## 🔵 The reliability idea — solved by subtraction instead

**The original argument, kept because it was a good one and the resolution is the interesting
part.** Steam's export names are mangled per build, so module discovery is structural and
inherently fragile — which is why Decky plugins feel unreliable after a Steam update: they break
silently. But `CLSTAMP` is readable from *both* `changelist.txt` on disk and the live page. So:
cache the resolved module map keyed by build stamp; on a stamp change, re-run every finder and
**diff against the cached map**, turning a silent break into

> *"Steam updated to build 10850000. 9 of 11 components re-found; `AppContextMenu` and
> `SliderField` not found — the context-menu entry is unavailable, use the F8 hotkey."*

~100 lines, nothing in Decky or Millennium does it, and it was called *the main reason to build
this rather than keep fighting the plugin*.

🔴 **It was built, it worked, and it has been deleted — because the problem it solved was created
entirely by deliverable B.** Every one of the eleven finders and all three features it graded were
injection targets. With B cut, the diff had nothing left to report: the desktop app discovers no
Steam modules at all. `SetCustomArtworkForApp` is bound by the CEF host, not shipped in Steam's
bundle, so **there is nothing a Steam update can silently take away.**

The lesson worth keeping is the shape of the trade. A monitoring system that reports on a fragile
subsystem is a real improvement over one that fails silently — but **removing the fragile
subsystem beats monitoring it**, and it is easy to get attached to the clever instrumentation and
forget to ask whether the thing being instrumented needs to exist. `Readiness` is now two fields
and one `typeof` check.

What survives of the idea: `cdp::mod` still reads `CLSTAMP` and Diagnostics still shows it, so a
bug report can name the build it was seen on. It gates nothing.

### 🟢 The diagnostics row audit, 2026-08-02 — four rows out, the version in

Run against the panel's own stated test — **does this help a bug report, or help the user act?**
The panel already carried the rule that matters (*"a green tick against a feature that does not
exist is worse than no panel"*); what it had not been asked is whether each *fact* earns its row.

| Removed | Why |
|---|---|
| `Steam running: yes/no` | A snapshot taken at startup and never refreshed, rendered as though current — and 🔴 **`sentinel_explanation` already says it**, as *"Live apply is on, but Steam isn't running"*. A duplicate in the worse form. |
| `Known apps: 2930` | A parser statistic with nothing to compare against. Only the `None` case explains anything, so only that case renders now. |
| `Cache: 4.2 MB` | Checked rather than assumed: `cache` is **LRU-capped at `DEFAULT_MAX_BYTES` = 512 MB** and self-manages, and `notes/uninstalling.md` already documents the directory. A number with nothing behind it. |
| `sentinel_present` | 🔴 **Dead across the boundary** — serialised, mirrored in `api.ts`, rendered nowhere. `sentinel_explanation` had absorbed it. |

`Found via` was kept but folded in beside the path: it explains exactly one failure — the wrong
Steam of two installs — and a bare registry key path standing alone reads as internals.

🔑 **The find that matters is the opposite direction: `Version` was missing, and
`notes/troubleshooting.md` told people to include it.** *"Include the version from Settings →
Diagnostics"* had never been true. Nothing catches this class — the docs described a panel that
nobody had read the panel against — and it is the single most useful line in a bug report.

`app_version` is `env!("CARGO_PKG_VERSION")`, so it reads **`0.0.0` on a development build**. That
is information, not a placeholder: the git tag is the source of truth and `scripts/set-version.ps1`
stamps it in during the release job, so `0.0.0` states truthfully "not built from a tag". Do not
"fix" it by hardcoding a number.

#### 🔴 Three more capability claims had gone stale the same way, all in release-facing copy

Found by sweeping the docs for the same question. Every one describes the product as it was before
a fix landed, and all three would have shipped in the release notes users read:

| Claim | Where | Reality |
|---|---|---|
| *"Steam games cannot have a custom icon… the control is disabled"* | `CHANGELOG.md` known limitations | The **reverted regression**, still recorded as a limitation. `commands/icon.rs`'s own header says icons for Steam games "go through `apply_asset` like every other slot… works and always has". |
| *"The **Sort** control is a native dropdown a controller cannot open"* | `CHANGELOG.md`, **and a whole `## One control a controller cannot reach` section** in `using/controller-and-keyboard.md` ending *"Replacing it is planned"* | Already replaced — `SortOptions` is three `useFocusItem` buttons. |
| *"Set the path yourself in **Settings**"* | `notes/troubleshooting.md` | 🔴 **There is no such control.** `SGDB_STEAM_PATH` is the only override, which `error.rs` says correctly — so the app was honest and the docs were not. |

🔑 **The pattern is one-directional and worth naming.** All four (these three plus the missing
version) are documentation that stopped tracking a change *in the product's favour* — a fix landed,
a limitation was lifted, a row was never added — and nothing failed, because prose does not compile.
`check-claims.sh` cannot reach any of them: they are claims about behaviour, not grep-able facts.
**Before a release, read the limitations list against the code, not against its own last version.**

#### 🟡 And three in `using/filters.mdx`, caught by the screenshot rather than by reading

A different and more useful discovery route: re-capturing `browse.png` put the filter panel and its
documentation side by side, and they disagreed three times. **A screenshot is a test of the prose
next to it** — none of these had been noticed by reading the page.

| It said | The panel shows |
|---|---|
| the styles are *"Alternate, Blurred, White Logo, **Material**, No Logo"* | **Minimal** — `STYLE_LABEL` maps `material` → `'Minimal'` deliberately, to match SteamGridDB's own UI, so "Material" is a name the reader cannot find anywhere |
| *"Everything starts **ticked** except Adult Content"* | **512×512 and 1024×1024 are also unticked**, deliberately — valid for grids but "not the shape Steam renders", per `DIMENSIONS`' own doc comment |
| *"Formats: Static (JPEG, PNG) and animated (WebP, APNG)"* | **five** ticks that are two independent things — three mime types, plus Animated/Static. Unticking WebP is not the same as unticking Animated, which is exactly the confusion the old wording invited |

The styles list is now written out per asset type, since it varies and the page said so without
saying how. Also added: **Reset filters** only appears once something is modified — a reader who
goes looking for it in the default state would otherwise report it missing.

#### 🔴 "Use **Wrong game?**" — four pages naming a control by the label it only wears when it fails

Found by the maintainer, and it is a third distinct class. Not stale, not overclaimed: **wrong from
the day it was written**, and wrong in the one situation the instruction exists for.

`FilterPanel.tsx` renders `{gameLabel ? \`Game: ${gameLabel}\` : 'Wrong game?'}`. So the button says
**"Wrong game?"** only when SteamGridDB matched *nothing*. A **wrong** match is still a match, so
the button reads **"Game: Portal 2"** — and the docs told the reader to look for a label that, in
that exact case, is not on screen.

Two things made it worse, and both were also stated wrongly:

- **"at the top" / "above the results".** The button is inside the `<details>` filter panel, which
  is **collapsed by default**. There is nothing to see until you expand Filters.
- **"If the header names a different game".** The heading is the *Steam* game's name and never
  changes. A wrong match is invisible there by construction, so the one check the page offered
  could not detect the thing it was checking for.

🔑 **The lesson is about labels that vary.** Documenting a control by the text it displays is fine
until the text is conditional, and then the docs are guaranteed to name the wrong one for whichever
branch the reader is in. Four pages had copied the same sentence. All four now describe the
*location* first and give both labels.

⚠️ **A screenshot would have caught this too**: `browse.png` shows the button reading **"Game: 007
First Light"**, directly contradicting the page that embeds it. Same as the `filters.mdx` finding
above — the image and the prose were never read against each other.

---

## Deliberate divergences from the Decky plugin

| Divergence | Why |
|---|---|
| **User supplies their own SGDB API key** | Decky's is hardcoded with an explicit *"attempting to use this in your own projects will cause you to be automatically banned and blacklisted"*. Non-negotiable. `[VERIFIED-SOURCE]` |
| **No in-Big-Picture UI at all** | 🔵 Cut 2026-07-31 — see the header. Decky gets one for free from `routerHook`; we would have had to inject into Steam's React tree, which is the entire source of the fragility this product exists to avoid. The couch case is served by controller-navigating the desktop window instead, launched from BPM as a non-Steam shortcut. |
| **Installed games, or everything `localconfig.vdf` knows (518 here)** | Fully offline; no Steam Web API. 🔴 **Neither is an ownership list** — `licensecache` is encrypted — so "All games" is labelled as such and never as "owned". |
| **One filter set for all five tabs**, not `filters_<type>` | Decky keys filters per asset type. Re-picking "no adult content" on five tabs is busywork, not a feature. Per-endpoint vocabularies (sizes, styles) are handled by **clamping at query time**, so a selection another tab cannot show is kept rather than discarded. |
| **No MOTD, donation modal, or tutorial video** | Decky-store furniture. The first-run API-key flow replaces the tutorial. |
| **Library style tweaks ship behind "Experimental"** | Square Capsules / Matching Recents / Capsule Glow patch Steam's own library rendering *globally* — the most fragile surface in the product. Same features, honest labelling, individually disableable. ⚠️ **Open question after the B cut:** these need the same structural module discovery that was just deleted, so shipping them means bringing that machinery back for them alone. Not yet decided. |
| **Plus, not in Decky: a diagnostics screen** | It reports the environment — **version**, Steam root, account, whether live apply is available — because almost every failure here is environmental. The build-stamped module map that used to sit beside it is gone; see the reliability section above, and the row audit below for the four rows that went with it. |
| **Plus, not in Decky: controller navigation on the desktop window** | Decky is already gamepad-native inside Steam's shell. We get there by making our own window drivable by a pad, which costs one spatial focus model and no Steam internals whatsoever. |

Matching Decky's restraint, explicitly **not** added: favorites, download history,
upload-to-SGDB, bulk apply, HeroBlur editing. Bulk apply in particular is the fastest route to
an SGDB rate-limit problem.
