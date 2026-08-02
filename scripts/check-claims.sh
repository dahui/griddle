#!/bin/sh
# Claims in CLAUDE.md that a machine can settle, enforced rather than trusted.
#
# Run by CI and by scripts/gate.ps1. Exit 0 = clean, 1 = a claim has gone stale.
#
# Why this exists: CLAUDE.md is read *instead of* re-deriving the environment, so a wrong line
# in it is more expensive than a wrong line of code -- code fails, a document persuades. Four of
# its claims have already caused real damage:
#
#   * "writing [an icon] has no route [for Steam apps]" -> a working feature was replaced with a
#     paragraph saying the feature was impossible.
#   * "icons need ... Steam shut down; then restart" -> a shutdown/relaunch flow built for a
#     problem `SteamClient.Apps.SetShortcutIcon` does not have.
#   * "four [MPL crates] are proc-macro-only" -> a licence audit called clean on a shipping dep.
#   * "librarycache ... contains no write at all" -> quietly false for weeks.
#
# The first three could only be caught by measuring. The fourth is grep-able, and so are the
# rules below. Anything needing Steam, the network or an API key is deliberately out of scope:
# a guard that cannot run offline in CI is a guard that gets disabled.
#
# Each rule names the CLAUDE.md claim it defends, so a failure says what to go and fix.

set -eu
fail=0
note() { printf '  %s\n' "$1" >&2; }

DOC=CLAUDE.md

# -- 1. "`griddle-core` contains no CRC32 function at all" --------------------------------
#
# The folklore appid algorithm is disproven (four variants computed against the real file, none
# matched). The claim is that the way to never regress is for the function not to exist -- so a
# real identifier here, as opposed to prose about its absence, falsifies it.
#
# No trailing `\b`. The first version had one and matched *nothing* -- not the injected
# `fn crc32_of(...)` it was written to catch, and not the existing `crc32_ieee` prose it was
# written to tolerate, because `2` and `_` are both word characters so there is no boundary
# between them. It passed vacuously until it was fired against a real violation.
crc_hits() {
  grep -rniE 'crc[_]?32' crates/griddle-core/src \
     | grep -vE '^[^:]+:[0-9]+:[[:space:]]*(//|///|//!|\*)'
}
if crc_hits >/dev/null 2>&1; then
  note 'CLAUDE.md: "griddle-core contains no CRC32 function at all, so there is nothing to regress to"'
  crc_hits >&2
  fail=1
else
  echo "[ok] no CRC32 implementation in griddle-core"
fi

# -- 2. "steam::librarycache is read-only in shipping code" -------------------------------
#
# Steam re-downloads over that directory, so writing there achieves nothing and risks a user's
# cache. Test fixtures into a tempdir are fine and carry `boundary-ok`, exactly as the write
# boundary requires; an unannotated write is not.
if grep -nE 'fs::write|File::create|remove_file|remove_dir|OpenOptions' \
     crates/griddle-core/src/steam/librarycache.rs 2>/dev/null \
     | grep -v 'boundary-ok' >/dev/null 2>&1; then
  note 'CLAUDE.md: "steam::librarycache -- read-only in shipping code"'
  grep -nE 'fs::write|File::create|remove_file|remove_dir|OpenOptions' \
     crates/griddle-core/src/steam/librarycache.rs | grep -v 'boundary-ok' >&2
  fail=1
else
  echo "[ok] librarycache has no unannotated write"
fi

# -- 3. "ApiKey implements no Display and no Serialize" -----------------------------------
#
# The first and least forgettable of the three layers protecting the key: the plaintext type
# cannot be formatted into a log line or serialised into settings.json, so the DPAPI wrap is not
# something a later edit can skip. `#[derive(Serialize)]` on `ApiKey`, or a `Display` impl for
# it, removes that guarantee silently.
if grep -nE 'impl[^/]*(fmt::)?Display[^/]*for\s+ApiKey|derive\([^)]*\bSerialize\b[^)]*\)\s*\]?\s*(pub\s+)?struct\s+ApiKey' \
     crates/griddle-core/src/sgdb/key.rs >/dev/null 2>&1; then
  note 'CLAUDE.md: "ApiKey implements no Serialize ... it has no Display either"'
  fail=1
else
  echo "[ok] ApiKey has neither Display nor Serialize"
fi

# -- 4. The comment policy: no emoji markers in source ------------------------------------
#
# CLAUDE.md keeps its emoji; source does not. `.css` is included on purpose -- the readability
# pass reported them removed while three survived in `styles.css`, because the verification grep
# named only .rs/.ts/.tsx. A guard that repeats the original blind spot is worse than none.
if grep -rn '🔴\|🟢\|🔵\|⚠️\|🔑' crates apps packages \
     --include='*.rs' --include='*.ts' --include='*.tsx' --include='*.css' \
     2>/dev/null | grep -v node_modules >/dev/null 2>&1; then
  note 'CLAUDE.md: the comment policy -- emoji markers belong in CLAUDE.md, not in source'
  grep -rn '🔴\|🟢\|🔵\|⚠️\|🔑' crates apps packages \
     --include='*.rs' --include='*.ts' --include='*.tsx' --include='*.css' \
     2>/dev/null | grep -v node_modules >&2
  fail=1
else
  echo "[ok] no emoji markers in source"
fi

# -- 5. The module maps list every command module -----------------------------------------
#
# This is the drift that actually happened: `commands/icon.rs` and `commands/logo.rs` were added
# and the table in CLAUDE.md kept listing eight modules while ten existed. Nothing noticed,
# because a table is not compiled.
for f in crates/griddle-app/src/commands/*.rs; do
  n=$(basename "$f" .rs)
  [ "$n" = "mod" ] && continue
  if ! grep -q "\`$n\`" "$DOC"; then
    note "CLAUDE.md: the griddle-app module table does not mention commands/$n.rs"
    fail=1
  fi
done
[ "$fail" -eq 0 ] && echo "[ok] every command module appears in CLAUDE.md"

# -- 6. Every write-boundary module named in the doc still exists --------------------------
#
# CLAUDE.md names four writers plus `fsutil`. If one is renamed, the prose keeps naming a module
# that is gone -- and the write boundary is the single most load-bearing claim in the document.
for m in grid/store steam/shortcuts settings fsutil cache; do
  if [ ! -e "crates/griddle-core/src/$m.rs" ] && [ ! -e "crates/griddle-core/src/$m/mod.rs" ]; then
    note "CLAUDE.md names '$m' as a sanctioned writer, but no such module exists"
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  printf '\nclaims check FAILED -- CLAUDE.md and the code disagree.\n' >&2
  printf 'Fix the code, or correct the document. Do not leave them disagreeing.\n' >&2
  exit 1
fi
echo "claims check passed"
