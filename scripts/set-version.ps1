# Stamp a version into every manifest that carries one.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\set-version.ps1 1.2.3
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\set-version.ps1 -Show
#
# The git tag is the source of truth for Griddle's version, so nothing in the repository is kept
# up to date by hand -- every manifest reads 0.0.0 on main and the release job stamps the tag in
# before building. That keeps the four files from drifting apart, which is the failure this
# replaces: an installer whose Add/Remove Programs entry disagrees with its own About screen.
#
# Run by CI on a throwaway checkout. Running it locally dirties your tree; `git checkout` the
# four files afterwards, or use -Show to see what it would do.
#
# ASCII-only, like the other scripts here: PowerShell's encoding defaults mangle non-ASCII on a
# read/write round-trip, and this script rewrites files.

[CmdletBinding(DefaultParameterSetName = 'Set')]
param(
    [Parameter(ParameterSetName = 'Set', Position = 0, Mandatory = $true)]
    [string]$Version,

    [Parameter(ParameterSetName = 'Show', Mandatory = $true)]
    [switch]$Show
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

# Each entry is a file plus the single line that carries its version. The patterns are anchored
# to the *first* occurrence of a key at the start of a line, because every one of these files
# also mentions versions elsewhere -- dependency requirements in Cargo.toml, dependency versions
# in package.json. A greedy pattern rewrites those instead and the mistake compiles.
$targets = @(
    @{
        Path    = 'Cargo.toml'
        Pattern = '(?m)^version = "[^"]*"'
        Format  = 'version = "{0}"'
        # The workspace version. Both crates inherit it with `version.workspace = true`, so this
        # one line covers them. Under [workspace.package], which is why the ^ anchor is safe:
        # dependency versions in this file are all inline table values, never line-initial.
    },
    @{
        Path    = 'crates/griddle-app/tauri.conf.json'
        Pattern = '(?m)^  "version": "[^"]*"'
        Format  = '  "version": "{0}"'
        # What NSIS shows in Add/Remove Programs, and what the installer filename carries.
    },
    @{
        Path    = 'package.json'
        Pattern = '(?m)^  "version": "[^"]*"'
        Format  = '  "version": "{0}"'
    },
    @{
        Path    = 'apps/desktop/package.json'
        Pattern = '(?m)^  "version": "[^"]*"'
        Format  = '  "version": "{0}"'
    },
    @{
        Path    = 'packages/shared/package.json'
        Pattern = '(?m)^  "version": "[^"]*"'
        Format  = '  "version": "{0}"'
    }
)

if ($Show) {
    foreach ($t in $targets) {
        $full = Join-Path $root $t.Path
        $text = [System.IO.File]::ReadAllText($full)
        $m = [regex]::Match($text, $t.Pattern)
        $found = if ($m.Success) { $m.Value.Trim() } else { '(NOT FOUND)' }
        "{0,-40} {1}" -f $t.Path, $found
    }
    exit 0
}

# Refuse a version the release job would not have produced. A malformed tag must fail here rather
# than yield a release named after a typo -- and this is the only place that can catch it before
# the artifacts are built and uploaded.
if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') {
    Write-Host "not a valid semver version: '$Version'" -ForegroundColor Red
    Write-Host "expected <major>.<minor>.<patch> with an optional -prerelease suffix"
    exit 1
}

# tauri.conf.json is schema-validated and rejects a version with a pre-release suffix in some
# bundlers, so the numeric core is used there. The release name still carries the full tag.
$numeric = ($Version -split '-')[0]

foreach ($t in $targets) {
    $full = Join-Path $root $t.Path
    if (-not (Test-Path $full)) { throw "missing file: $($t.Path)" }

    $text = [System.IO.File]::ReadAllText($full)
    $matches = [regex]::Matches($text, $t.Pattern)

    # Both directions are errors. Zero means the file changed shape and this script silently
    # stamped nothing; more than one means the pattern is catching something it should not, and
    # picking the first would be a coin flip.
    if ($matches.Count -ne 1) {
        Write-Host "$($t.Path): expected exactly 1 version line, found $($matches.Count)" -ForegroundColor Red
        exit 1
    }

    $value = if ($t.Path -like '*tauri.conf.json') { $numeric } else { $Version }
    $updated = [regex]::Replace($text, $t.Pattern, ($t.Format -f $value))

    # Written as UTF-8 without a BOM and with the original line endings preserved by virtue of
    # only replacing within a line. A BOM here would break the encoding check.
    [System.IO.File]::WriteAllText($full, $updated, (New-Object System.Text.UTF8Encoding $false))
    Write-Host "  $($t.Path) -> $value"
}

Write-Host "stamped $Version" -ForegroundColor Green
