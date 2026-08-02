# Regenerate every logo asset from the two source files.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\logo.ps1
#
#   assets\griddle-logo.jpg   the wordmark  -> the app header, the welcome screen, docs, README
#   assets\griddle-icon.jpg   the "G" mark  -> the .ico, the docs favicon
#
# Two sources rather than one, and that is the point of this revision. An earlier logo stacked its
# wordmark under a mascot badge, so a header wanted a wide crop and an icon wanted a square one and
# both had to be cut out of the same picture -- which showed, because the word overlapped the badge
# and there was no clean edge to cut along. Purpose-drawn art for each shape means nothing here
# crops or rearranges anything: both images are used whole.
#
# Uses System.Drawing rather than ImageMagick or Pillow, neither of which is installed and both of
# which would be a new dependency for a script that runs about once a year. `screenshots.ps1`
# already takes the same approach.
#
# ASCII-only, like the other scripts here.

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Add-Type -AssemblyName System.Drawing

# -- The two sources, and what makes each one hard ---------------------------------------------
#
# They need opposite predicates, so the background mode is a property of the source, not a global.
#
#   logo   white ground, and the lettering is WHITE TOO -- measured at 252.9 against a 254.1
#          ground. A 1.2 separation, so no threshold can tell them apart and no colour key can
#          ever work. What saves it is that the fill runs inward from the border and the splash
#          stops it: the letters survive because they are *enclosed*, not because they are
#          distinguishable. If a future source lets the ground touch a letter, the word will
#          hollow out -- check the cutout, not the thumbnail.
#
#   icon   dark charcoal ground with a soft vignette baked around the mark. The vignette is
#          background but sits well above it in luminance, which is what the alpha floor is for.
$SOURCES = @(
    @{ Name = 'logo'; File = 'assets\griddle-logo.jpg'; Light = $true },
    @{ Name = 'icon'; File = 'assets\griddle-icon.jpg'; Light = $false }
)

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;

public static class LogoKit {
    public static double Lum(int r, int g, int b) { return 0.2126 * r + 0.7152 * g + 0.0722 * b; }

    /// <summary>Luminance of every border pixel, sorted -- the input to threshold derivation.</summary>
    public static double[] BorderRing(Bitmap src) {
        int w = src.Width, h = src.Height;
        var vals = new List<double>();
        var d = src.LockBits(new Rectangle(0, 0, w, h), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        int[] px = new int[w * h];
        for (int y = 0; y < h; y++)
            System.Runtime.InteropServices.Marshal.Copy(
                (IntPtr)(d.Scan0.ToInt64() + y * d.Stride), px, y * w, w);
        src.UnlockBits(d);
        Action<int> add = i => vals.Add(Lum((px[i] >> 16) & 255, (px[i] >> 8) & 255, px[i] & 255));
        for (int x = 0; x < w; x++) { add(x); add((h - 1) * w + x); }
        for (int y = 0; y < h; y++) { add(y * w); add(y * w + w - 1); }
        var arr = vals.ToArray();
        Array.Sort(arr);
        return arr;
    }

    /// <summary>
    /// Where the alpha ramp starts and ends, from the image's own border.
    ///
    /// A percentile, never the extreme. Deriving from the minimum put one candidate's thresholds
    /// at lo=-26 / hi=16 and keyed the entire image away, because a handful of very dark pixels
    /// survived in its border. One outlier must not set the threshold for the other 99 percent.
    /// </summary>
    public static void Thresholds(Bitmap src, bool light, out double lo, out double hi) {
        var ring = BorderRing(src);
        const double RAMP = 42;
        double p = ring[(int)Math.Floor((ring.Length - 1) * (light ? 0.15 : 0.85))];
        if (light) { hi = p - 2; lo = hi - RAMP; } else { lo = p + 2; hi = lo + RAMP; }
    }

    public static Bitmap Key(Bitmap src, bool light, double lo, double hi) {
        int w = src.Width, h = src.Height;
        var rect = new Rectangle(0, 0, w, h);
        int[] px = new int[w * h];
        var sd = src.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        for (int y = 0; y < h; y++)
            System.Runtime.InteropServices.Marshal.Copy(
                (IntPtr)(sd.Scan0.ToInt64() + y * sd.Stride), px, y * w, w);
        src.UnlockBits(sd);

        // The ground colour, averaged over the border. Needed to un-mix the edge pixels below:
        // every semi-transparent pixel is part background, and leaving that in is what puts a
        // halo around the artwork when it is placed on a different colour.
        long sr = 0, sg = 0, sb = 0; int n = 0;
        for (int x = 0; x < w; x++)
            foreach (int i in new[] { x, (h - 1) * w + x }) {
                int c = px[i]; sr += (c >> 16) & 255; sg += (c >> 8) & 255; sb += c & 255; n++;
            }
        for (int y = 0; y < h; y++)
            foreach (int i in new[] { y * w, y * w + w - 1 }) {
                int c = px[i]; sr += (c >> 16) & 255; sg += (c >> 8) & 255; sb += c & 255; n++;
            }
        int bgR = (int)(sr / n), bgG = (int)(sg / n), bgB = (int)(sb / n);

        // "Could be background." The fill still has to *reach* a pixel to clear it, and that is
        // the whole mechanism protecting the white lettering on the logo.
        Func<int, bool> passable = i => {
            int c = px[i];
            double l = Lum((c >> 16) & 255, (c >> 8) & 255, c & 255);
            return light ? l > lo : l < hi;
        };

        // Iterative. A recursive fill overflows the stack -- the filled region is most of a
        // 393,000-pixel image.
        bool[] outside = new bool[w * h];
        var q = new Queue<int>();
        Action<int> seed = i => { if (!outside[i] && passable(i)) { outside[i] = true; q.Enqueue(i); } };
        for (int x = 0; x < w; x++) { seed(x); seed((h - 1) * w + x); }
        for (int y = 0; y < h; y++) { seed(y * w); seed(y * w + w - 1); }
        while (q.Count > 0) {
            int i = q.Dequeue();
            int x = i % w, y = i / w;
            if (x > 0) seed(i - 1);
            if (x < w - 1) seed(i + 1);
            if (y > 0) seed(i - w);
            if (y < h - 1) seed(i + w);
        }

        var dst = new Bitmap(w, h, PixelFormat.Format32bppArgb);
        var dd = dst.LockBits(rect, ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
        int[] outPx = new int[w * h];
        for (int i = 0; i < px.Length; i++) {
            int c = px[i];
            int r = (c >> 16) & 255, g = (c >> 8) & 255, b = c & 255;
            if (!outside[i]) { outPx[i] = unchecked((int)0xFF000000) | (r << 16) | (g << 8) | b; continue; }

            double l = Lum(r, g, b);
            double a = light ? (hi - l) / (hi - lo) : (l - lo) / (hi - lo);

            // Floor. Below this the pixel is ground haze -- on the icon source, the vignette baked
            // around the mark, which the ramp otherwise leaves at alpha 0.05-0.10 and which renders
            // as a dark rectangular smudge behind the artwork. Real watercolour splatter sits well
            // above this, so nothing intended is lost.
            if (a <= 0.11) { outPx[i] = 0; continue; }
            if (a > 1) a = 1;

            // observed = true*a + bg*(1-a)  =>  true = (observed - bg*(1-a)) / a
            int ur = (int)Math.Round((r - bgR * (1 - a)) / a);
            int ug = (int)Math.Round((g - bgG * (1 - a)) / a);
            int ub = (int)Math.Round((b - bgB * (1 - a)) / a);
            outPx[i] = ((int)Math.Round(a * 255) << 24)
                     | ((ur < 0 ? 0 : ur > 255 ? 255 : ur) << 16)
                     | ((ug < 0 ? 0 : ug > 255 ? 255 : ug) << 8)
                     |  (ub < 0 ? 0 : ub > 255 ? 255 : ub);
        }
        for (int y = 0; y < h; y++)
            System.Runtime.InteropServices.Marshal.Copy(
                outPx, y * w, (IntPtr)(dd.Scan0.ToInt64() + y * dd.Stride), w);
        dst.UnlockBits(dd);
        return dst;
    }

    public static Rectangle ContentBounds(Bitmap bmp, int minAlpha) {
        int w = bmp.Width, h = bmp.Height;
        var d = bmp.LockBits(new Rectangle(0, 0, w, h), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        int[] px = new int[w * h];
        for (int y = 0; y < h; y++)
            System.Runtime.InteropServices.Marshal.Copy(
                (IntPtr)(d.Scan0.ToInt64() + y * d.Stride), px, y * w, w);
        bmp.UnlockBits(d);
        int minX = w, minY = h, maxX = -1, maxY = -1;
        for (int y = 0; y < h; y++)
            for (int x = 0; x < w; x++)
                if (((px[y * w + x] >> 24) & 255) >= minAlpha) {
                    if (x < minX) minX = x;
                    if (x > maxX) maxX = x;
                    if (y < minY) minY = y;
                    if (y > maxY) maxY = y;
                }
        return maxX < 0 ? Rectangle.Empty : new Rectangle(minX, minY, maxX - minX + 1, maxY - minY + 1);
    }

    /// <summary>
    /// Snap each colour channel to `levels` steps, leaving alpha alone.
    ///
    /// A size optimisation. The sources are JPEGs, so even their smooth areas arrive as thousands
    /// of nearly-identical colours -- close to the worst case for PNG, since deflate has nothing
    /// to repeat. The wordmark goes from 255 KB to 119 KB at 440px wide.
    ///
    /// 32 was checked, not assumed, and the check mattered more here than it did for the previous
    /// artwork: that was flat pixel art, where posterising is nearly free, while this is
    /// watercolour and is continuous gradient everywhere. A 3x zoom through the wash behind the
    /// letters is indistinguishable from the original at 64 and at 32 -- the paint texture
    /// dithers the steps for us.
    ///
    /// Alpha is deliberately excluded: quantising it would re-introduce a hard edge on the soft
    /// watercolour rim the ramp in `Key` exists to preserve.
    /// </summary>
    public static Bitmap Posterize(Bitmap src, int levels) {
        int w = src.Width, h = src.Height;
        var rect = new Rectangle(0, 0, w, h);
        var dst = new Bitmap(w, h, PixelFormat.Format32bppArgb);
        var sd = src.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        var dd = dst.LockBits(rect, ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
        int[] px = new int[w * h];
        for (int y = 0; y < h; y++)
            System.Runtime.InteropServices.Marshal.Copy(
                (IntPtr)(sd.Scan0.ToInt64() + y * sd.Stride), px, y * w, w);

        double step = 255.0 / (levels - 1);
        for (int i = 0; i < px.Length; i++) {
            int c = px[i];
            int a = (c >> 24) & 255;
            if (a == 0) { px[i] = 0; continue; }
            int r = (int)(Math.Round(((c >> 16) & 255) / step) * step);
            int g = (int)(Math.Round(((c >> 8) & 255) / step) * step);
            int b = (int)(Math.Round((c & 255) / step) * step);
            px[i] = (a << 24) | ((r > 255 ? 255 : r) << 16)
                  | ((g > 255 ? 255 : g) << 8) | (b > 255 ? 255 : b);
        }
        for (int y = 0; y < h; y++)
            System.Runtime.InteropServices.Marshal.Copy(
                px, y * w, (IntPtr)(dd.Scan0.ToInt64() + y * dd.Stride), w);
        src.UnlockBits(sd); dst.UnlockBits(dd);
        return dst;
    }

    public static Bitmap Crop(Bitmap src, Rectangle r) {
        var dst = new Bitmap(r.Width, r.Height, PixelFormat.Format32bppArgb);
        using (var g = Graphics.FromImage(dst)) {
            g.CompositingMode = CompositingMode.SourceCopy;
            g.DrawImage(src, new Rectangle(0, 0, r.Width, r.Height), r, GraphicsUnit.Pixel);
        }
        return dst;
    }

    public static Bitmap Scale(Bitmap src, int w, int h) {
        var dst = new Bitmap(w, h, PixelFormat.Format32bppArgb);
        using (var g = Graphics.FromImage(dst)) {
            g.CompositingQuality = CompositingQuality.HighQuality;
            g.InterpolationMode = InterpolationMode.HighQualityBicubic;
            g.PixelOffsetMode = PixelOffsetMode.HighQuality;
            g.DrawImage(src, 0, 0, w, h);
        }
        return dst;
    }

    /// <summary>Fit into a square, centred, preserving aspect and transparency.</summary>
    public static Bitmap FitSquare(Bitmap src, int size, double pad) {
        var dst = new Bitmap(size, size, PixelFormat.Format32bppArgb);
        double avail = size * (1.0 - 2 * pad);
        double scale = Math.Min(avail / src.Width, avail / src.Height);
        int dw = Math.Max(1, (int)Math.Round(src.Width * scale));
        int dh = Math.Max(1, (int)Math.Round(src.Height * scale));
        using (var g = Graphics.FromImage(dst)) {
            g.CompositingQuality = CompositingQuality.HighQuality;
            g.InterpolationMode = InterpolationMode.HighQualityBicubic;
            g.PixelOffsetMode = PixelOffsetMode.HighQuality;
            g.DrawImage(src, (size - dw) / 2, (size - dh) / 2, dw, dh);
        }
        return dst;
    }

    /// <summary>
    /// Write a multi-resolution .ico whose entries are PNGs.
    ///
    /// Hand-rolled because System.Drawing cannot produce one: `Icon.Save` round-trips a single
    /// image and reduces the alpha channel to a 1-bit mask. The container is a 6-byte header plus
    /// one 16-byte directory entry per image, so writing it directly is less work than the
    /// workarounds. PNG-compressed entries are what every modern generator emits and what Windows
    /// has read since Vista.
    ///
    /// Do not verify the result with `Icon.ToBitmap`: it mis-reports PNG-compressed entries,
    /// claiming alpha in empty corners and decoding the 256 frame at 128x128. Read the embedded
    /// PNG payloads directly instead.
    /// </summary>
    public static void WriteIco(Bitmap[] images, string path) {
        var blobs = new List<byte[]>();
        foreach (var img in images)
            using (var ms = new MemoryStream()) { img.Save(ms, ImageFormat.Png); blobs.Add(ms.ToArray()); }

        using (var fs = new FileStream(path, FileMode.Create, FileAccess.Write))
        using (var bw = new BinaryWriter(fs)) {
            bw.Write((short)0); bw.Write((short)1); bw.Write((short)images.Length);
            int offset = 6 + 16 * images.Length;
            for (int i = 0; i < images.Length; i++) {
                int side = images[i].Width;
                bw.Write((byte)(side >= 256 ? 0 : side));   // 0 encodes 256
                bw.Write((byte)(side >= 256 ? 0 : side));
                bw.Write((byte)0); bw.Write((byte)0);
                bw.Write((short)1); bw.Write((short)32);
                bw.Write(blobs[i].Length); bw.Write(offset);
                offset += blobs[i].Length;
            }
            foreach (var b in blobs) bw.Write(b);
        }
    }
}
'@

function Save-Png([Drawing.Bitmap]$bmp, [string]$path) {
    $dir = Split-Path $path -Parent
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }
    $bmp.Save($path, [Drawing.Imaging.ImageFormat]::Png)
    $rel = $path.Substring($root.Length + 1)
    "  {0,-46} {1,5}x{2,-5} {3,8:N0} bytes" -f $rel, $bmp.Width, $bmp.Height, (Get-Item $path).Length
}

# -- Cut both sources out --------------------------------------------------------------------
$cut = @{}
foreach ($s in $SOURCES) {
    $path = Join-Path $root $s.File
    if (-not (Test-Path $path)) { throw "no source artwork at $path" }
    $orig = [Drawing.Bitmap]::FromFile($path)
    $lo = 0.0; $hi = 0.0
    [LogoKit]::Thresholds($orig, $s.Light, [ref]$lo, [ref]$hi)
    $keyed = [LogoKit]::Key($orig, $s.Light, $lo, $hi)
    $cut[$s.Name] = [LogoKit]::Crop($keyed, [LogoKit]::ContentBounds($keyed, 10))
    "{0,-5} {1,4}x{2,-4} ground {3,-5} lo {4,6:N1} hi {5,6:N1} -> {6}x{7}" -f `
        $s.Name, $orig.Width, $orig.Height, $(if ($s.Light) { 'light' } else { 'dark' }),
        $lo, $hi, $cut[$s.Name].Width, $cut[$s.Name].Height
    $keyed.Dispose(); $orig.Dispose()
}

Write-Host "`nwrote:" -ForegroundColor Cyan

# -- The app icon ------------------------------------------------------------------------------
# 2 percent padding: at 16px a mark bled to the very edge looks clipped against a taskbar.
$iconSizes = 16, 24, 32, 48, 64, 128, 256
$iconArt = [LogoKit]::Posterize($cut['icon'], 32)
$frames = $iconSizes | ForEach-Object { [LogoKit]::FitSquare($iconArt, $_, 0.02) }
$icoPath = Join-Path $root 'crates\griddle-app\icons\icon.ico'
[LogoKit]::WriteIco($frames, $icoPath)
"  {0,-46} {1,25:N0} bytes" -f 'crates\griddle-app\icons\icon.ico', (Get-Item $icoPath).Length
Save-Png $frames[$frames.Count - 1] (Join-Path $root 'crates\griddle-app\icons\icon.png')
Save-Png ([LogoKit]::FitSquare($iconArt, 256, 0.02)) (Join-Path $root 'docs\public\favicon.png')

# -- The wordmark ------------------------------------------------------------------------------
#
# One image, two encodings, because the two consumers have different constraints.
#
# The app copy is bundled into the exe, so every kilobyte is permanent: 440px wide is 2.2x the
# 200px the welcome screen draws it at, which is ample for a HiDPI panel, and posterising takes it
# to 119 KB from 630. Never resampled *up* -- an earlier revision wrote a 482px source out at 640
# and the extra pixels were invented.
$appLogo = [LogoKit]::Posterize([LogoKit]::Scale($cut['logo'], 440,
    [int](440.0 * $cut['logo'].Height / $cut['logo'].Width)), 32)
Save-Png $appLogo (Join-Path $root 'apps\desktop\src\assets\logo.png')

# The docs copy stays at native width: Astro re-encodes it to webp and derives its own sizes, so
# this is a source rather than what ships, and the README renders it at 300px straight from git.
Save-Png ([LogoKit]::Posterize($cut['logo'], 32)) (Join-Path $root 'docs\src\assets\logo.png')

foreach ($f in $frames) { $f.Dispose() }
$iconArt.Dispose(); $appLogo.Dispose()
foreach ($k in $cut.Keys) { $cut[$k].Dispose() }
Write-Host "`ndone." -ForegroundColor Green
