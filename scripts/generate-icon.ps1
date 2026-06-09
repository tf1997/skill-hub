Add-Type -AssemblyName System.Drawing

$ErrorActionPreference = "Stop"

$iconsDir = Resolve-Path "src-tauri\icons"
$size = 256
$bmp = New-Object System.Drawing.Bitmap $size, $size
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias

$rect = New-Object System.Drawing.Rectangle 0, 0, $size, $size
$bg = [System.Drawing.Drawing2D.LinearGradientBrush]::new(
  $rect,
  [System.Drawing.Color]::FromArgb(0, 166, 181),
  [System.Drawing.Color]::FromArgb(35, 132, 255),
  45
)
$g.FillRectangle($bg, $rect)

$shine = [System.Drawing.Drawing2D.LinearGradientBrush]::new(
  $rect,
  [System.Drawing.Color]::FromArgb(80, 255, 255, 255),
  [System.Drawing.Color]::FromArgb(0, 255, 255, 255),
  90
)
$g.FillRectangle($shine, 0, 0, $size, 72)

$pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(245, 255, 255, 255)), 18
$pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round

$g.DrawLine($pen, 72, 88, 128, 56)
$g.DrawLine($pen, 128, 56, 184, 88)
$g.DrawLine($pen, 72, 88, 128, 120)
$g.DrawLine($pen, 128, 120, 184, 88)
$g.DrawLine($pen, 72, 128, 128, 160)
$g.DrawLine($pen, 128, 160, 184, 128)
$g.DrawLine($pen, 72, 168, 128, 200)
$g.DrawLine($pen, 128, 200, 184, 168)

$pngPath = Join-Path $iconsDir "icon.png"
$bmp.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)

$ms = New-Object System.IO.MemoryStream
$bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
$png = $ms.ToArray()

$icoPath = Join-Path $iconsDir "icon.ico"
$fs = [System.IO.File]::Create($icoPath)
$bw = New-Object System.IO.BinaryWriter $fs
$bw.Write([UInt16]0)
$bw.Write([UInt16]1)
$bw.Write([UInt16]1)
$bw.Write([Byte]0)
$bw.Write([Byte]0)
$bw.Write([Byte]0)
$bw.Write([Byte]0)
$bw.Write([UInt16]1)
$bw.Write([UInt16]32)
$bw.Write([UInt32]$png.Length)
$bw.Write([UInt32]22)
$bw.Write($png)
$bw.Close()
$fs.Close()

$ms.Dispose()
$g.Dispose()
$bg.Dispose()
$shine.Dispose()
$pen.Dispose()
$bmp.Dispose()

Write-Host "generated icon.png and icon.ico"
