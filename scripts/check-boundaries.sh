#!/bin/sh
# Architectural boundaries, enforced rather than trusted.
#
# Run by CI and by scripts/gate.ps1. Exit 0 = clean, 1 = violation.
#
#   1. `sgdb-core` stays free of `tauri` / `anyhow` — all logic must be headless.
#   2. Only `grid::store`, `steam::shortcuts` and `settings` may write files.
#   3. `steam://flushconfig` is never invoked.
#
# Rule 2 is the important one: this project's failure mode is corrupting a user's
# irreplaceable Steam config, so the write surface is kept small enough to audit by grep.

set -eu
fail=0
note() { printf '  %s\n' "$1" >&2; }

# ── 1. Dependency boundary ────────────────────────────────────────────────────────────────
if grep -nE '^\s*(tauri|anyhow)\s*=' crates/sgdb-core/Cargo.toml >/dev/null 2>&1; then
  note "sgdb-core must stay free of tauri/anyhow — all logic is headless"
  grep -nE '^\s*(tauri|anyhow)\s*=' crates/sgdb-core/Cargo.toml >&2
  fail=1
else
  echo "[ok] sgdb-core has no tauri/anyhow"
fi

# ── 2. Write boundary ─────────────────────────────────────────────────────────────────────
#
# Only *production* code is scanned. Everything from the first `#[cfg(test)]` onward is a
# test module, and tests legitimately write fixtures into temp directories — an earlier
# version of this check flagged exactly that and would have trained us to ignore it.
violations=$(
  for f in $(find crates/sgdb-core/src -name '*.rs'); do
    # `settings/mod.rs` as well as `settings.rs`: the module gained a `dpapi` submodule and
    # became a directory. Only the mod file is exempt -- `settings/dpapi.rs` encrypts bytes and
    # has no business touching the filesystem, so it stays inside the boundary.
    case "$f" in
      */grid/store.rs|*/steam/shortcuts.rs|*/settings.rs|*/settings/mod.rs) continue ;;
    esac
    awk -v file="$f" '
      /#\[cfg\(test\)\]/ { exit }
      /fs::write|File::create|remove_file|remove_dir|OpenOptions/ {
        print file ":" NR ": " $0
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
# filtered because the ban is *documented* in sgdb-core/src/lib.rs, and a naive grep flags
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
