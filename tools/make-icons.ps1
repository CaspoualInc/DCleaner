# Génère les icônes (app + paquet Win11) : carré bleu Windows uni avec un mini treemap blanc.
Add-Type -AssemblyName System.Drawing
$root = Split-Path $PSScriptRoot
$icons = Join-Path $root 'src-tauri\icons'
$assets = Join-Path $root 'package\Assets'
New-Item -ItemType Directory -Force $icons, $assets | Out-Null

function New-Icon([int]$s, [string]$path) {
    $bmp = New-Object System.Drawing.Bitmap $s, $s
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.Clear([System.Drawing.Color]::Transparent)
    $r = [int]($s * 0.22)
    $gp = New-Object System.Drawing.Drawing2D.GraphicsPath
    $gp.AddArc(0, 0, 2 * $r, 2 * $r, 180, 90); $gp.AddArc($s - 2 * $r - 1, 0, 2 * $r, 2 * $r, 270, 90)
    $gp.AddArc($s - 2 * $r - 1, $s - 2 * $r - 1, 2 * $r, 2 * $r, 0, 90); $gp.AddArc(0, $s - 2 * $r - 1, 2 * $r, 2 * $r, 90, 90)
    $gp.CloseFigure()
    $br = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(0, 103, 192))
    $g.FillPath($br, $gp)
    $w = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(235, 255, 255, 255))
    $w2 = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(150, 255, 255, 255))
    $p = $s * 0.2; $m = $s * 0.03; $inner = $s - 2 * $p
    $g.FillRectangle($w, $p, $p, $inner * 0.58 - $m, $inner)
    $g.FillRectangle($w2, $p + $inner * 0.58 + $m, $p, $inner * 0.42 - $m, $inner * 0.55 - $m)
    $g.FillRectangle($w2, $p + $inner * 0.58 + $m, $p + $inner * 0.55 + $m, $inner * 0.42 - $m, $inner * 0.45 - $m)
    $g.Dispose()
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

New-Icon 256 (Join-Path $icons 'icon.png')
New-Icon 150 (Join-Path $assets 'logo150.png')
New-Icon 44 (Join-Path $assets 'logo44.png')

# ICO contenant le PNG 256 (format accepté depuis Vista)
$png = [IO.File]::ReadAllBytes((Join-Path $icons 'icon.png'))
$ms = New-Object IO.MemoryStream; $bw = New-Object IO.BinaryWriter $ms
$bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]1)
$bw.Write([byte]0); $bw.Write([byte]0); $bw.Write([byte]0); $bw.Write([byte]0)
$bw.Write([uint16]1); $bw.Write([uint16]32); $bw.Write([uint32]$png.Length); $bw.Write([uint32]22)
$bw.Write($png); $bw.Flush()
[IO.File]::WriteAllBytes((Join-Path $icons 'icon.ico'), $ms.ToArray())
