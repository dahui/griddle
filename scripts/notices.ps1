# Regenerate THIRD-PARTY-NOTICES.txt from the dependency graph.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\notices.ps1
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\notices.ps1 -Check
#
# Griddle ships under Apache-2.0, which says nothing about the ~280 crates statically linked
# into the release binary. MIT and BSD both require their copyright notices travel with the
# code we redistribute, and one MPL-2.0 crate (option-ext) requires a pointer to its source.
# That obligation is discharged by this file, and only if it is current.
#
# -Check is what CI runs: regenerate into a temp file and diff. A dependency that arrives with
# an unlisted licence then fails the build instead of shipping unattributed -- the failure mode
# this guards against is silent, because nothing about an out-of-date notices file looks wrong.
#
# ASCII-only, like gate.ps1: PowerShell's encoding defaults mangle non-ASCII on a round-trip.

param([switch]$Check)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$target = Join-Path $root 'THIRD-PARTY-NOTICES.txt'
$template = Join-Path $root 'about.hbs'

if (-not (Get-Command cargo-about -ErrorAction SilentlyContinue)) {
    # The `cli` feature is not default, and without it `cargo install cargo-about` reports
    # success while installing no binary at all.
    Write-Host "cargo-about is not installed. Run:" -ForegroundColor Yellow
    Write-Host "    cargo install cargo-about --locked --features cli"
    exit 1
}

$out = if ($Check) { Join-Path ([System.IO.Path]::GetTempPath()) 'griddle-notices-check.txt' } else { $target }

Push-Location $root
try {
    cargo about generate $template -o $out
    if ($LASTEXITCODE -ne 0) { throw "cargo about failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

if (-not $Check) {
    Write-Host "wrote $target" -ForegroundColor Green
    exit 0
}

if (-not (Test-Path $target)) {
    Write-Host "THIRD-PARTY-NOTICES.txt does not exist. Run scripts\notices.ps1" -ForegroundColor Red
    exit 1
}

# Compared as bytes rather than by line, so a line-ending change is caught too -- the file is
# committed and a CRLF round-trip would otherwise show up as a spurious CI failure later.
$a = [System.IO.File]::ReadAllBytes($target)
$b = [System.IO.File]::ReadAllBytes($out)
# Lines are read here, before the temp file goes, so the failure branch below can diff.
$generatedLines = [System.IO.File]::ReadAllLines($out)
Remove-Item $out -ErrorAction SilentlyContinue

if ($a.Length -ne $b.Length -or [System.Convert]::ToBase64String($a) -ne [System.Convert]::ToBase64String($b)) {
    Write-Host "THIRD-PARTY-NOTICES.txt is out of date." -ForegroundColor Red
    Write-Host "  committed: $($a.Length) bytes"
    Write-Host "  generated: $($b.Length) bytes"

    # Say *what* differs, not just that something does. A bare "out of date" is unactionable
    # when the check runs somewhere the maintainer cannot reproduce -- which is how this failed
    # for a whole afternoon. Section headings alone are usually enough to name the crate.
    $committed = [System.IO.File]::ReadAllLines($target)
    $delta = Compare-Object $committed $generatedLines | Select-Object -First 40
    if ($delta) {
        Write-Host "`nfirst differing lines ('<=' committed only, '=>' generated only):"
        $delta | ForEach-Object { Write-Host ("  {0} {1}" -f $_.SideIndicator, $_.InputObject) }
    }

    Write-Host "`nRun scripts\notices.ps1 and commit the result."
    exit 1
}

Write-Host "THIRD-PARTY-NOTICES.txt is current" -ForegroundColor Green
