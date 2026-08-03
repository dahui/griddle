# Capture the documentation screenshots from the release build.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\screenshots.ps1
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\screenshots.ps1 -Welcome
#
# Screenshots go stale silently when the UI changes -- nothing fails, the docs just start
# describing a version nobody has. So this is a script rather than a remembered procedure, and
# re-running it is a release-checklist step.
#
# The window is moved to a fixed size first, so successive captures line up and a diff of the
# images is meaningful. Clicks are synthesised at window-relative coordinates.
#
# Driven by mouse rather than keyboard on purpose: SendKeys reaches the Tauri window but the
# WebView2 child does not take keyboard focus from SetForegroundWindow, so the page never sees
# the keystroke. Clicks land wherever the cursor is and do not care about focus.
#
# ASCII-only, like the other scripts here.
#
# # Two modes, because the API key decides which screens exist
#
# The four main captures need a key -- without one the app never leaves first run. `-Welcome`
# needs the opposite. They cannot be one pass, and each mode ASSERTS its precondition rather than
# creating it.
#
# That asymmetry is deliberate and is the whole safety story of this script. An earlier harness
# reached first run by moving `settings.json` aside and restoring it afterwards; the maintainer's
# DPAPI-sealed key was destroyed by it twice -- once when a cycle failed mid-run, and once when a
# LATER run restored a stale backup over good settings. A key sealed by DPAPI cannot be recovered
# from anything on disk.
#
# So this script never moves, copies, deletes or writes `settings.json`. To capture the welcome
# screen, remove your key in the app first (Settings -> "Remove it") and put it back afterwards.
# That is a deliberate manual step, and it is the point: the person who can undo it is the one
# doing it.

[CmdletBinding()]
param(
    # Capture the first-run welcome screen instead of the four main screens. Requires that no API
    # key is stored; this script will not remove one for you.
    [switch]$Welcome
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$exe = Join-Path $root 'target\release\griddle-app.exe'
$outDir = Join-Path $root 'docs\src\assets'

if (-not (Test-Path $exe)) {
    Write-Host "no release build at $exe" -ForegroundColor Red
    Write-Host "run: bun run app:release"
    exit 1
}
New-Item -ItemType Directory -Force $outDir | Out-Null

# Which screens the app will show us. Read-only -- see the header.
$settingsPath = Join-Path $env:APPDATA 'Griddle\settings.json'
$hasKey = $false
if (Test-Path $settingsPath) {
    $hasKey = $null -ne (Get-Content $settingsPath -Raw | ConvertFrom-Json).api_key_protected
}

if ($Welcome -and $hasKey) {
    Write-Host "an API key is stored, so Griddle will not show the welcome screen" -ForegroundColor Red
    Write-Host "remove it in the app (Settings -> 'Remove it'), re-run, then paste it back."
    Write-Host "this script deliberately will not touch settings.json -- see its header."
    exit 1
}
if (-not $Welcome -and -not $hasKey) {
    # Without this the run "succeeds" and writes four identical copies of the welcome screen over
    # the real docs images, because every click lands on a screen that has none of those controls.
    Write-Host "no API key is stored, so Griddle opens on first run and the four main screens" -ForegroundColor Red
    Write-Host "are unreachable. Paste your key into the app first, then re-run."
    Write-Host "(for the welcome screen itself, use -Welcome)"
    exit 1
}

# Default filters are a PRECONDITION, checked -- they used to be a click, and that click was
# dangerous.
#
# This script used to press "Reset filters" before expanding the panel, so the shot showed the
# product rather than whoever ran it. But that button renders only when the filters ARE modified
# (`{modified && ...}` in FilterPanel.tsx), and the panel is seeded open on the same condition. So
# on a machine already at defaults -- the common case -- the panel was shut, the button did not
# exist, and the click went to fixed coordinates that now land inside the artwork grid.
#
# Clicking artwork APPLIES it. A normalisation step that silently rewrites a game's capsule is a
# far worse failure than the one it was preventing, and nothing about it would have been visible
# in the output.
#
# Asserting is strictly better here: it cannot misfire, it needs no coordinates, and the remedy is
# one button press by the person already sitting in front of the app.
#
# Only in the main mode -- the welcome screen has no filter panel to show anyone's choices in.
if (-not $Welcome -and (Test-Path $settingsPath)) {
    $filters = (Get-Content $settingsPath -Raw | ConvertFrom-Json).filters
    if ($null -ne $filters) {
        Write-Host "filters are not at their defaults, so the screenshots would show your choices" -ForegroundColor Red
        Write-Host "open Griddle, pick any game, expand Filters, press 'Reset filters', then re-run."
        Write-Host "(a null 'filters' key in $settingsPath is what this checks)"
        exit 1
    }
}

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Win {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int t, bool repaint);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int size);
    // DWMWA_EXTENDED_FRAME_BOUNDS
    public const int FRAME = 9;
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    public const uint DOWN = 0x0002, UP = 0x0004, WHEEL = 0x0800;
}
'@

# Made DPI-aware before anything is measured or captured.
#
# Without this, `MoveWindow` and `GetWindowRect` speak virtualised coordinates while
# `CopyFromScreen` reads physical pixels, so the crop lands beside the window and slices the
# right-hand edge off -- and the geometry printed above looks perfectly correct throughout.
[void][Win]::SetProcessDPIAware()

# A 16:10 window: big enough that a full row of capsules fits, small enough to embed in a page
# without being downscaled to mush.
$W = 1280; $H = 820

$proc = Start-Process $exe -PassThru
Start-Sleep -Seconds 3
$proc.WaitForInputIdle(5000) | Out-Null
$hwnd = $proc.MainWindowHandle
if ($hwnd -eq [IntPtr]::Zero) { Write-Host "no window appeared" -ForegroundColor Red; exit 1 }

[void][Win]::MoveWindow($hwnd, 60, 60, $W, $H, $true)
[void][Win]::SetForegroundWindow($hwnd)
# The library has to load and its artwork has to arrive from disk and the CDN.
Start-Sleep -Seconds 8

# The whole window, frame included, and both numbers from the same call.
#
# Mixing them is the trap: a first version took the position from the window rect and the size
# from the client rect, which quietly captured a region 15px narrower and 30px higher than the
# window -- so every image had the title bar in it and the right-hand controls cut off, and it
# read as the app overflowing rather than as a bad crop.
#
# The title bar is kept deliberately. These are screenshots of a Windows application, and the
# frame is what says so.
# DWM's extended frame bounds, not `GetWindowRect`. On Windows 10 and 11 the window rect
# includes an invisible resize border several pixels wide on three sides, so cropping to it
# leaves a sliver of whatever is behind the window along each edge.
$win = New-Object Win+RECT
if ([Win]::DwmGetWindowAttribute($hwnd, [Win]::FRAME, [ref]$win, 16) -ne 0) {
    [void][Win]::GetWindowRect($hwnd, [ref]$win)
}
$originX = $win.L
$originY = $win.T
$width = $win.R - $win.L
$height = $win.B - $win.T

# Clicks are given in *client* coordinates, so they stay meaningful if the frame changes.
$client = New-Object Win+POINT
[void][Win]::ClientToScreen($hwnd, [ref]$client)

Write-Host "  window at $originX,$originY size ${width}x${height}; client origin $($client.X),$($client.Y)" -ForegroundColor DarkGray

function Capture([string]$name, [switch]$CropAboveLastPanel) {
    $bmp = New-Object Drawing.Bitmap $width, $height
    $g = [Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($originX, $originY, 0, 0, $bmp.Size)
    $g.Dispose()

    if ($CropAboveLastPanel) { $bmp = CropAboveLastPanel $bmp $name }

    $path = Join-Path $outDir "$name.png"
    $bmp.Save($path, [Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Host ("  {0,-12} {1}" -f $name, $path) -ForegroundColor Green
}

# Cut the image off above the last panel that starts in frame.
#
# This exists for exactly one image and one reason: the Settings screen's bottom panel is
# Diagnostics, and its Account row is the Steam account id. Whether that row lands above or below
# the window's bottom edge depends on how long the copy in the panels ABOVE it happens to be --
# so trimming a sentence anywhere on that screen can push a private value into a published
# screenshot, silently.
#
# It has already come within 13px: shortening the API-key paragraph pulled two Diagnostics rows
# into frame, leaving the account id about 10px below the cut. That is an accident, not a
# guarantee, and the previous guarantee was a `Write-Host` warning asking a human to look.
#
# Panels are `<section>`s with a 1px `--line` border (#2c2f3d), so the last one is findable by
# colour. Scanning bottom-up, the first row that is mostly border colour is that panel's top edge.
# Failing loudly matters more than cropping well: a miss here publishes the id.
function CropAboveLastPanel([Drawing.Bitmap]$bmp, [string]$name) {
    $edge = -1
    for ($y = $bmp.Height - 1; $y -gt 200; $y--) {
        $hits = 0
        for ($x = 200; $x -lt 1000; $x += 4) {
            $p = $bmp.GetPixel($x, $y)
            # --line is #2c2f3d. Allow a little slack for subpixel blending at the border.
            if ([Math]::Abs($p.R - 0x2c) -le 6 -and [Math]::Abs($p.G - 0x2f) -le 6 -and
                [Math]::Abs($p.B - 0x3d) -le 6) { $hits++ }
        }
        if ($hits -gt 180) { $edge = $y; break }
    }
    if ($edge -lt 300) {
        throw "${name}: could not find the last panel's top border (got $edge). Refusing to save, because the crop is what keeps the Steam account id out of this image."
    }
    $keep = $edge - 12
    Write-Host ("    cropped {0} to {1}px, above the last panel at y={2}" -f $name, $keep, $edge) -ForegroundColor DarkGray
    $out = New-Object Drawing.Bitmap $bmp.Width, $keep
    $g2 = [Drawing.Graphics]::FromImage($out)
    $g2.DrawImage($bmp, 0, 0)
    $g2.Dispose()
    $bmp.Dispose()
    return $out
}

function Click([int]$x, [int]$y) {
    [void][Win]::SetCursorPos(($client.X + $x), ($client.Y + $y))
    Start-Sleep -Milliseconds 250
    [Win]::mouse_event([Win]::DOWN, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [Win]::mouse_event([Win]::UP, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 1200
}

function Scroll([int]$x, [int]$y, [int]$notches) {
    [void][Win]::SetCursorPos(($client.X + $x), ($client.Y + $y))
    Start-Sleep -Milliseconds 200
    for ($i = 0; $i -lt [Math]::Abs($notches); $i++) {
        [Win]::mouse_event([Win]::WHEEL, 0, 0, [uint32](-120 * [Math]::Sign($notches)), [IntPtr]::Zero)
        Start-Sleep -Milliseconds 80
    }
    Start-Sleep -Milliseconds 600
}

Write-Host "capturing:" -ForegroundColor Cyan

# The welcome screen needs no navigation at all -- it is what the app opens on when there is no
# key, which the precondition above has already established. So this mode synthesises no input
# whatsoever: nothing to mis-aim, and nothing that could land on a control.
if ($Welcome) {
    Capture 'welcome'
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    Write-Host "done. Now paste your API key back into Griddle." -ForegroundColor Yellow
    exit 0
}

Capture 'library'

# Click targets are read off a captured image and converted to client coordinates, which is what
# `Click` takes: client X = image X - ($client.X - $win.L), and likewise for Y. Getting that
# conversion wrong is silent -- the click lands somewhere harmless and the next capture is just
# a duplicate of the previous one, which is exactly how the first attempt failed.
#
# Every value below was re-derived from fresh captures on 2026-08-02, after the 64px -> 84px
# wordmark pushed the whole page down: the Settings tab had drifted 23px and was landing 1px
# ABOVE the tab. That is the drift this file's own header warns about, and it had already
# happened.
Click 152 467      # the first game tile
Start-Sleep -Seconds 5

# Expand the Filters panel. Safe to click unconditionally *because* the precondition above
# guarantees the filters are at their defaults, which means the panel is seeded shut -- so this
# is always an open, never a close. Without that guarantee it would be a coin flip.
Click 103 422      # the Filters summary
Start-Sleep -Seconds 2
Capture 'browse'

Click 551 351      # the Current tab, last in the asset tab bar
Start-Sleep -Seconds 4
Capture 'current'

# Straight to Settings from the asset browser. There used to be a "back to the library" click
# first, which was pointless twice over: the Settings tab is in the same nav row on both screens,
# and the *nav* tab cannot return to the list anyway -- `App` renders the browser whenever
# `selected` is set, so only the view's own "<- Library" button clears it.
Click 184 216      # the Settings tab
Start-Sleep -Seconds 3
# Cropped above the Diagnostics panel, which carries the Steam account id. That is enforced by
# `CropAboveLastPanel` rather than left to the window happening to cut in the right place -- see
# its comment for why the window is not a reliable boundary.
#
# Two ways of scrolling to those rows failed *silently* and are worth recording: synthesised
# `mouse_event` wheel messages never reached the WebView2 child, and resizing the window between
# captures did not change what was captured. Both produced an image identical to the previous one
# rather than an error, so check that successive captures actually differ before believing a
# navigation step worked.
Capture 'settings' -CropAboveLastPanel

Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Write-Host "done. Check each image before committing -- the click targets are positional." -ForegroundColor Yellow
Write-Host "in particular: 'browse' must show an EXPANDED filter panel." -ForegroundColor Yellow
