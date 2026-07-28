#!/bin/sh
# Block secrets from entering git history.
#
# Run by .githooks/pre-commit on staged content, and by CI over the tree and over history.
#
# Why this exists: the SteamGridDB API key is a *user* secret. Every endpoint 401s without
# one, so a key will inevitably be pasted into a terminal, a test, or a config while
# developing. This makes "it accidentally got committed" a build failure rather than a
# discovery six months later.
#
#   scripts/check-secrets.sh              # staged changes (pre-commit)
#   scripts/check-secrets.sh --all        # every tracked file (CI)
#   scripts/check-secrets.sh --history    # every commit ever (CI)
#
# Exit 0 = clean, 1 = secret found.
#
# ── Known keys are stored as SHA-256, never as literals ────────────────────────────────────
# A checker that contains the strings it forbids trips over itself: the first version of this
# file held decky's key verbatim and blocked its own commit, along with the CLAUDE.md section
# explaining the rule. Hashing means this file can name a specific key without containing it,
# and there is nothing here for a future scan to flag.

set -eu

MODE="${1:-staged}"

# The maintainer's SteamGridDB key. A real secret.
KNOWN_KEY_SHA="212755d6f2268a51d6f727ec519420e060ee8edd5b16d8ffb99f50924f8453f6"

# decky-steamgriddb's hardcoded key. NOT confidential — it is published in a public repo and
# has since been revoked (401 as of 2026-07-27). It is blocked for a different reason: it is
# not ours to ship, and its own source comment says using it elsewhere earns a ban. Blocking
# it stops someone "solving" the first-run key prompt by pasting it in.
DECKY_KEY_SHA="1c5cef7552ba6acc6f251859ec191fb6ef9c32555fc10aee1d0e2be5183b2fc2"

fail=0
note() { printf '  %s\n' "$1" >&2; }

have_sha256() { command -v sha256sum >/dev/null 2>&1; }

# Hash every 32-hex token on stdin and report any that matches a known key.
# `where` is a human-readable location for the message.
scan_tokens() {
  where="$1"
  have_sha256 || return 0
  grep -oE "[0-9a-fA-F]{32}" 2>/dev/null | tr 'A-F' 'a-f' | sort -u | while read -r tok; do
    [ -n "$tok" ] || continue
    h=$(printf '%s' "$tok" | sha256sum | cut -d' ' -f1)
    case "$h" in
      "$KNOWN_KEY_SHA") printf 'MAINTAINER_KEY %s\n' "$where" ;;
      "$DECKY_KEY_SHA") printf 'DECKY_KEY %s\n' "$where" ;;
    esac
  done
}

report_token_hits() {
  hits="$1"
  [ -n "$hits" ] || return 0
  echo "$hits" | while read -r kind where; do
    case "$kind" in
      MAINTAINER_KEY) note "SECRET: $where — the maintainer's SteamGridDB API key" ;;
      DECKY_KEY) note "SECRET: $where — decky-steamgriddb's key (revoked, and not ours to ship)" ;;
    esac
  done
}

# ── History mode ───────────────────────────────────────────────────────────────────────────
if [ "$MODE" = "--history" ]; then
  hits=$(git log -p --all 2>/dev/null | scan_tokens "git history" | sort -u)
  if [ -n "$hits" ]; then
    report_token_hits "$hits"
    note ""
    note "A secret removed in a later commit is still a leaked secret. Rotate the key,"
    note "then rewrite history (git filter-repo) before pushing anywhere public."
    exit 1
  fi
  if git log -p --all 2>/dev/null \
       | grep -qE "Authorization[\"']?[[:space:]]*[:=][[:space:]]*[\"']?Bearer[[:space:]]+[0-9a-fA-F]{32}"; then
    note "SECRET: a literal Bearer token appears somewhere in git history"
    exit 1
  fi
  echo "history clean"
  exit 0
fi

# ── File modes ─────────────────────────────────────────────────────────────────────────────
if [ "$MODE" = "--all" ]; then
  files=$(git ls-files)
else
  files=$(git diff --cached --name-only --diff-filter=ACM)
fi

[ -z "$files" ] && exit 0

for f in $files; do
  [ -f "$f" ] || continue
  # Lockfiles and binaries are full of legitimate hex and are never hand-edited.
  case "$f" in
    Cargo.lock|bun.lock|*.lock|*.ico|*.png|*.jpg|*.jpeg|*.webp|*.gif) continue ;;
    # This file necessarily discusses key formats. It holds hashes, not keys.
    scripts/check-secrets.sh) continue ;;
  esac
  grep -Iq . "$f" 2>/dev/null || continue   # skip binary

  # 1. A 32-hex literal assigned to an obviously-secret-looking name.
  pat="(sgdb[_-]?api[_-]?key|api[_-]?key|apikey|auth[_-]?token|bearer)[[:space:]]*[:=][[:space:]]*[\"'][0-9a-fA-F]{32}[\"']"
  if grep -nEi "$pat" "$f" >/dev/null 2>&1; then
    note "SECRET: $f — a 32-hex literal assigned to an API-key-looking name"
    grep -nEi "$pat" "$f" | head -3 >&2
    fail=1
  fi

  # 2. A literal token in an Authorization header.
  if grep -nEi "Authorization[\"']?[[:space:]]*[:=][[:space:]]*[\"']?Bearer[[:space:]]+[0-9a-fA-F]{32}" "$f" >/dev/null 2>&1; then
    note "SECRET: $f — literal Bearer token in an Authorization header"
    fail=1
  fi

  # 3. Any known key by hash. Catches a bare paste with no surrounding context — a comment,
  #    a README example, a test fixture.
  hits=$(scan_tokens "$f" < "$f" | sort -u)
  if [ -n "$hits" ]; then
    report_token_hits "$hits"
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  cat >&2 <<'EOF'

COMMIT BLOCKED — a secret was detected.

The SteamGridDB API key is a per-user secret and must never be committed. At runtime it is
stored DPAPI-wrapped under %APPDATA%; in development, pass it via the SGDB_API_KEY environment
variable or a gitignored .env file.

If this is a false positive (a public asset hash, a test vector), narrow the pattern in
scripts/check-secrets.sh rather than bypassing the hook.
EOF
  exit 1
fi

exit 0
