#!/bin/sh
# Architectural boundaries, enforced rather than trusted.
#
# Run by CI and by scripts/gate.ps1. Exit 0 = clean, 1 = violation.
#
#   1. `griddle-core` stays free of `tauri` / `anyhow` — all logic must be headless.
#   2. Only `grid::store`, `steam::shortcuts`, `settings`, `cache` and the shared `fsutil` may
#      write files.
#   3. `steam://flushconfig` is never invoked.
#
# Rule 2 is the important one: this project's failure mode is corrupting a user's
# irreplaceable Steam config, so the write surface is kept small enough to audit by grep.

set -eu
fail=0
note() { printf '  %s\n' "$1" >&2; }

# ── 1. Dependency boundary ────────────────────────────────────────────────────────────────
if grep -nE '^\s*(tauri|anyhow)\s*=' crates/griddle-core/Cargo.toml >/dev/null 2>&1; then
  note "griddle-core must stay free of tauri/anyhow — all logic is headless"
  grep -nE '^\s*(tauri|anyhow)\s*=' crates/griddle-core/Cargo.toml >&2
  fail=1
else
  echo "[ok] griddle-core has no tauri/anyhow"
fi

# ── 2. Write boundary ─────────────────────────────────────────────────────────────────────
#
# Every line of every file is scanned. An earlier version stopped at the first `#[cfg(test)]`,
# on the theory that tests come last and legitimately write fixtures into temp directories --
# but that is a heuristic about *layout*, so a write placed after the test module was invisible.
# Demonstrated: appending `std::fs::write` to the end of `appid.rs` passed the check.
#
# Parsing Rust well enough to know what is test code is not worth it: `#[cfg(test)]` can sit on
# a module, a function, or a single struct field, and brace-counting breaks on the last of
# those. So instead, flag every write and require legitimate exceptions to say so with
# `boundary-ok`. Every one of those is a test fixture written into a tempdir, so every write in
# the codebase is either in a sanctioned module or explicitly annotated -- and greppable.
#
# Note what happened when the big test modules moved into `*_tests.rs` siblings: twenty fixture
# writes that had been invisible inside exempt files started being flagged. That is the check
# working. They are annotated rather than exempted by filename, because a `*_tests.rs` pattern
# would be another heuristic about layout -- exactly what the paragraph above rejects.
violations=$(
  for f in $(find crates/griddle-core/src -name '*.rs'); do
    # `settings/mod.rs` as well as `settings.rs`: the module gained a `dpapi` submodule and
    # became a directory. Only the mod file is exempt -- `settings/dpapi.rs` encrypts bytes and
    # has no business touching the filesystem, so it stays inside the boundary.
    #
    # `cache.rs` writes only under %LOCALAPPDATA%\<app>\cache, which we created and which is
    # disposable -- deleting it costs a re-download. The boundary exists to keep writes to the
    # user's *irreplaceable Steam config* auditable, and that is a different category. Every
    # path in that module is derived from its own root and every filename is a hash, so it
    # cannot write outside its directory; there is a test for exactly that.
    #
    # `fsutil.rs` is the shared atomic write the other three now call. It is not a fourth writer:
    # it has no idea what a grid directory or a settings file is, and every path it touches was
    # chosen by a caller that is itself on this list. Consolidating the temp-write-fsync-rename
    # dance was the point -- three copies were three chances to lose the fsync, which is invisible
    # until a crash leaves a correctly-named empty file where the user's artwork used to be.
    case "$f" in
      */grid/store.rs|*/steam/shortcuts.rs|*/settings.rs|*/settings/mod.rs|*/cache.rs|*/fsutil.rs) continue ;;
    esac
    # `boundary-ok` exempts the line it appears on *or* the line immediately below it, so a
    # long call can be annotated on its own comment line rather than with a trailing comment.
    awk -v file="$f" '
      {
        if ($0 ~ /boundary-ok/) { skip_next = 1; next }
        if (skip_next)          { skip_next = 0; next }
        if ($0 ~ /fs::write|File::create|remove_file|remove_dir|OpenOptions/) {
          print file ":" NR ": " $0
        }
      }
    ' "$f"
  done
)
if [ -n "$violations" ]; then
  note "file writes outside grid::store / steam::shortcuts / settings:"
  echo "$violations" >&2
  fail=1
else
  echo "[ok] writes confined to the sanctioned modules"
fi

# ── 3. steam://flushconfig ────────────────────────────────────────────────────────────────
#
# It has historically made Steam forget its library folder locations. Comment lines are
# filtered because the ban is *documented* in griddle-core/src/lib.rs, and a naive grep flags
# the very text that forbids it.
hits=$(grep -rn 'steam://flushconfig' \
         --include='*.rs' --include='*.ts' --include='*.tsx' \
         crates packages apps 2>/dev/null \
       | grep -vE '^[^:]+:[0-9]+:[[:space:]]*(//|\*|#)' || true)
if [ -n "$hits" ]; then
  note "steam://flushconfig is banned — it can make Steam forget its library folders"
  echo "$hits" >&2
  fail=1
else
  echo "[ok] no steam://flushconfig"
fi

exit "$fail"
