# Makes the small, cropped logo images the BitDinghy window uses, from the full brand logos in
# `assets/logos`. Run it again whenever the brand logos change:
#
#   powershell -File tools\make-app-images.ps1
#
# The brand logos are 1800 pixels wide with a lot of empty margin. Putting them in the program as
# they are would make it bigger and slower to start, so this script
#   1. crops away the transparent margin,
#   2. scales the result to a fixed height (128 pixels: sharp when shown 44-64 pixels tall, even
#      on a high-DPI screen),
#   3. saves it into crates/bitdinghy-app/assets/.

param(
    [int]$Height = 128
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class OpaqueBounds
{
    // The smallest rectangle that contains every pixel whose alpha is at least minAlpha.
    public static Rectangle Find(Bitmap bitmap, int minAlpha)
    {
        var data = bitmap.LockBits(new Rectangle(0, 0, bitmap.Width, bitmap.Height),
                                   ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        try
        {
            var bytes = new byte[Math.Abs(data.Stride) * data.Height];
            Marshal.Copy(data.Scan0, bytes, 0, bytes.Length);
            int minX = bitmap.Width, minY = bitmap.Height, maxX = -1, maxY = -1;
            for (int y = 0; y < data.Height; y++)
            {
                for (int x = 0; x < data.Width; x++)
                {
                    if (bytes[y * data.Stride + x * 4 + 3] >= minAlpha)
                    {
                        if (x < minX) minX = x;
                        if (x > maxX) maxX = x;
                        if (y < minY) minY = y;
                        if (y > maxY) maxY = y;
                    }
                }
            }
            if (maxX < 0) return new Rectangle(0, 0, bitmap.Width, bitmap.Height);
            return new Rectangle(minX, minY, maxX - minX + 1, maxY - minY + 1);
        }
        finally
        {
            bitmap.UnlockBits(data);
        }
    }
}
"@ -ReferencedAssemblies System.Drawing

function New-AppLogo([string]$Source, [string]$Destination) {
    $image = [System.Drawing.Bitmap]::FromFile($Source)
    try {
        $box = [OpaqueBounds]::Find($image, 8)
        $scale = $Height / $box.Height
        $width = [int][Math]::Round($box.Width * $scale)
        $output = New-Object System.Drawing.Bitmap($width, $Height, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            $graphics = [System.Drawing.Graphics]::FromImage($output)
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
            $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $graphics.Clear([System.Drawing.Color]::Transparent)
            $target = New-Object System.Drawing.Rectangle(0, 0, $width, $Height)
            $graphics.DrawImage($image, $target, $box.X, $box.Y, $box.Width, $box.Height, [System.Drawing.GraphicsUnit]::Pixel)
            $graphics.Dispose()
            $output.Save($Destination, [System.Drawing.Imaging.ImageFormat]::Png)
        }
        finally { $output.Dispose() }
        "{0}: cropped {1}x{2} -> {3}x{4}" -f (Split-Path -Leaf $Destination), $box.Width, $box.Height, $width, $Height
    }
    finally { $image.Dispose() }
}

$assets = Join-Path $root 'crates\bitdinghy-app\assets'
New-Item -ItemType Directory -Force -Path $assets | Out-Null

# "-dark" in the brand file name means "for a dark background" (white lettering).
New-AppLogo (Join-Path $root 'assets\logos\horizontal-logo-dark.png')  (Join-Path $assets 'logo-on-dark.png')
New-AppLogo (Join-Path $root 'assets\logos\horizontal-logo-light.png') (Join-Path $assets 'logo-on-light.png')
