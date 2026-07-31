# Pre-push gate. Run this before pushing; it is what CI runs, in the same order.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\gate.ps1
#
# The point is that a red CI run should be impossible to produce from a green local gate.
# Every check that could differ between here and CI is delegated to the same script, rather
# than reimplemented -- an inline copy has already drifted once.
#
# Note this file stays ASCII-only on purpose: PowerShell's encoding defaults mangle non-ASCII
# on a read/write round-trip, and the encoding check itself was corrupted that way once.

$ErrorActionPreference = 'Continue'
$failed = @()

function Step {
    param([string]$Name, [scriptblock]$Block)
    Write-Host "== $Name" -ForegroundColor Cyan
    $global:LASTEXITCODE = 0
    try {
        & $Block
        if ($LASTEXITCODE -ne 0) { throw "exit code $LASTEXITCODE" }
        Write-Host "   ok" -ForegroundColor Green
    } catch {
        Write-Host "   FAILED: $_" -ForegroundColor Red
        $script:failed += $Name
    }
}

# `sh` is not on PATH in a bare PowerShell session -- it ships with Git for Windows. Resolve
# it from git's own location rather than assuming the shell environment.
function Get-Sh {
    $onPath = Get-Command sh -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    $git = Get-Command git -ErrorAction SilentlyContinue
    if ($git) {
        $gitRoot = Split-Path (Split-Path $git.Source -Parent) -Parent
        foreach ($rel in @("bin\sh.exe", "usr\bin\sh.exe")) {
            $candidate = Join-Path $gitRoot $rel
            if (Test-Path $candidate) { return $candidate }
        }
    }
    throw "could not find sh.exe (install Git for Windows, or run the .sh checks directly)"
}
$sh = Get-Sh

Step "secret scan"             { & $sh scripts/check-secrets.sh --all }
Step "architecture boundaries" { & $sh scripts/check-boundaries.sh }
Step "encoding"                { python scripts/check-encoding.py }
Step "cargo fmt"               { cargo fmt --all -- --check }
Step "cargo clippy"            { cargo clippy -q -p griddle-core --all-targets -- -D warnings }
Step "cargo test"              { cargo test -q -p griddle-core }
# griddle-app was clippy'd but never tested, here or in CI. `clippy --all-targets` compiles a
# test without running it, so the tests in commands.rs had never once executed.
Step "cargo test (app)"        { cargo test -q -p griddle-app }

$bun = Join-Path $env:USERPROFILE ".bun\bin\bun.exe"

Step "bun test" {
    if (Test-Path $bun) { & $bun test } else { Write-Host "   (bun not installed, skipping)" }
}

# CI runs this and the gate did not, which is precisely the drift the gate exists to prevent:
# `bun-types` was referenced by tsconfig but never installed, so typecheck had been failing
# while every local run stayed green.
Step "tsc typecheck" {
    if (Test-Path $bun) { & $bun run typecheck } else { Write-Host "   (bun not installed, skipping)" }
}

Write-Host ""
if ($failed.Count -gt 0) {
    Write-Host "GATE FAILED: $($failed -join ', ')" -ForegroundColor Red
    exit 1
}
Write-Host "GATE PASSED" -ForegroundColor Green
