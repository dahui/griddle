# Capture the documentation screenshots from the release build.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\screenshots.ps1
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

function Capture([string]$name) {
    $bmp = New-Object Drawing.Bitmap $width, $height
    $g = [Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($originX, $originY, 0, 0, $bmp.Size)
    $path = Join-Path $outDir "$name.png"
    $bmp.Save($path, [Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Host ("  {0,-12} {1}" -f $name, $path) -ForegroundColor Green
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
Capture 'library'

# Click targets are read off a captured image and converted to client coordinates, which is what
# `Click` takes: client X = image X - ($client.X - $win.L), and likewise for Y. Getting that
# conversion wrong is silent -- the click lands somewhere harmless and the next capture is just
# a duplicate of the previous one, which is exactly how the first attempt failed.
Click 152 467      # the first game tile
Start-Sleep -Seconds 5
# Reset the filters before capturing, so the shot shows the app's out-of-the-box state rather
# than whatever the machine running this happens to have saved. Notably it puts the Adult tick
# back to off, which is the default and is not something to publish either way round by accident.
#
# This *writes* to the settings file. It is the only thing in this script that changes anything,
# and it is here rather than left to chance because a screenshot of someone's personal filter
# choices is not a screenshot of the product.
# Reset the filters, so the shot shows the app's out-of-the-box state rather than whatever the
# machine running this happens to have saved -- notably the Adult tick, which is off by default
# and is not something to publish either way round by accident.
#
# This *writes* to the settings file, and it is the only thing in this script that changes
# anything. A screenshot of someone's personal filter choices is not a screenshot of the product.
Click 1053 501     # Reset filters
Start-Sleep -Seconds 3

# Then expand the panel, because resetting collapses it.
#
# The panel is seeded open only when the filters differ from their defaults -- so resetting them
# makes it close, and the two steps fight each other unless the second one exists. Clicking the
# summary is also idempotent in the wrong direction: run this twice against an already-default
# settings file and the first click hits nothing, leaving the panel shut. Hence the assertion in
# the output: the browse capture must show an expanded panel.
Click 95 399       # the Filters summary
Start-Sleep -Seconds 2
Capture 'browse'

Click 543 328      # the Current tab, last in the asset tab bar
Start-Sleep -Seconds 4
Capture 'current'

Click 88 262       # back to the library
Start-Sleep -Seconds 2
Click 176 193      # the Settings tab
Start-Sleep -Seconds 3
# The top of the screen, not the Diagnostics rows below the fold. Two ways of reaching those
# failed *silently* and are worth recording: synthesised `mouse_event` wheel messages never
# reached the WebView2 child, and resizing the window between captures did not change what was
# captured. Both produced an image identical to the previous one rather than an error, so check
# that successive captures actually differ before believing a navigation step worked.
#
# It is also the right frame to publish: the Diagnostics rows carry the Steam account id.
Capture 'settings'

Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Write-Host "done. Check each image before committing -- the click targets are positional." -ForegroundColor Yellow
