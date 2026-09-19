$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$images = @()
foreach ($size in @(16,32,48,256)) {
    $bitmap = [Drawing.Bitmap]::new($size,$size)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.ScaleTransform($size / 64.0,$size / 64.0)
    $background = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(29,71,113))
    $paper = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(239,247,255))
    $ink = [Drawing.Pen]::new([Drawing.Color]::FromArgb(72,131,188),3)
    $accent = [Drawing.Pen]::new([Drawing.Color]::FromArgb(241,166,75),5)
    try {
        $graphics.FillRectangle($background,2,2,60,60)
        $graphics.FillRectangle($paper,15,10,35,44)
        $graphics.DrawLine($ink,22,21,43,21)
        $graphics.DrawLine($ink,22,29,43,29)
        $graphics.DrawLine($ink,22,37,37,37)
        $graphics.DrawLine($accent,10,18,10,49)
        $stream = [IO.MemoryStream]::new()
        try {
            $bitmap.Save($stream,[Drawing.Imaging.ImageFormat]::Png)
            $images += @{ Size = $size; Bytes = $stream.ToArray() }
        } finally { $stream.Dispose() }
    } finally {
        $accent.Dispose(); $ink.Dispose(); $paper.Dispose(); $background.Dispose()
        $graphics.Dispose(); $bitmap.Dispose()
    }
}
$output = Join-Path (Split-Path $PSScriptRoot -Parent) 'assets\rstpd.ico'
$writer = [IO.BinaryWriter]::new([IO.File]::Create($output))
try {
    $writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$images.Count)
    $offset = 6 + 16 * $images.Count
    foreach ($image in $images) {
        $dimension = if ($image.Size -eq 256) { 0 } else { $image.Size }
        $writer.Write([byte]$dimension); $writer.Write([byte]$dimension)
        $writer.Write([byte]0); $writer.Write([byte]0)
        $writer.Write([uint16]1); $writer.Write([uint16]32)
        $writer.Write([uint32]$image.Bytes.Length); $writer.Write([uint32]$offset)
        $offset += $image.Bytes.Length
    }
    foreach ($image in $images) { $writer.Write([byte[]]$image.Bytes) }
} finally { $writer.Dispose() }
