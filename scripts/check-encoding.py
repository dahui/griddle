#!/usr/bin/env python3
"""Detect encoding corruption in tracked text files.

Run by CI and by scripts/gate.ps1.  Exit 0 = clean, 1 = corruption found.

Windows PowerShell's Get-Content/Set-Content round-trip silently mangles non-ASCII: em
dashes in Rust doc comments became the three characters U+00E2 U+20AC U+201D, and a careless
repair turned them into U+0022 U+201D.  The result compiles, reads as noise, and is invisible
in a diff unless you look for it.

**The patterns are written as hex escapes, never as literal mojibake.**  An earlier version of
this check embedded the corrupted characters directly and was itself corrupted when the file
was rewritten by PowerShell -- the detector broke in exactly the way it was meant to detect.
Hex escapes are inert no matter what rewrites this file.
"""

import subprocess
import sys

# UTF-8 encodings of the sequences that appear when UTF-8 is decoded as Latin-1/CP1252.
#   U+00E2 U+20AC  -- the start of a mangled em dash / quote
#   U+00C3 U+201A  -- mangled U+00C2
#   U+00C3 U+00A9  -- mangled e-acute
BAD = {
    "UTF-8 read as Latin-1 (mangled dash/quote)": b"\xc3\xa2\xe2\x82\xac",
    "double-encoded U+00C2": b"\xc3\x83\xe2\x80\x9a",
    "double-encoded e-acute": b"\xc3\x83\xc2\xa9",
    # The specific damage a bad repair produced: '"' immediately followed by U+201D.
    "botched mojibake repair": b'\x22\xe2\x80\x9d',
}

SUFFIXES = (".rs", ".ts", ".tsx", ".js", ".md", ".json", ".toml", ".yml", ".sh", ".py", ".css")


def tracked_files():
    """Tracked files **and** untracked ones git would let you add.

    `git ls-files` alone covers only tracked files, which meant a brand-new file could be
    corrupted and pass this check right up until the moment it was committed -- precisely when
    the damage becomes permanent. That happened: a PowerShell `Get-Content -Raw` /
    `Set-Content` round-trip mangled every em-dash in a new, still-untracked module, and this
    script reported "encoding clean".

    `--others --exclude-standard` adds untracked-but-not-ignored files, so a file is checked
    from the moment it exists rather than from the moment it is staged.
    """
    out = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    # `--cached --others` can list the same path twice; dedupe while keeping order.
    seen, files = set(), []
    for f in out.splitlines():
        if f.endswith(SUFFIXES) and f not in seen:
            seen.add(f)
            files.append(f)
    return files


def main() -> int:
    failures = []
    for path in tracked_files():
        try:
            with open(path, "rb") as fh:
                raw = fh.read()
        except OSError:
            continue

        for label, needle in BAD.items():
            idx = raw.find(needle)
            if idx != -1:
                line = raw.count(b"\n", 0, idx) + 1
                failures.append(f"{path}:{line}: {label}")

        # Anything that is not valid UTF-8 at all is also corruption.
        try:
            raw.decode("utf-8")
        except UnicodeDecodeError as exc:
            failures.append(f"{path}: not valid UTF-8 ({exc.reason} at byte {exc.start})")

    if failures:
        print("encoding corruption -- rewrite the file as UTF-8:", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1

    print("[ok] encoding clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
