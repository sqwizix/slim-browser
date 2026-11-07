$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$assetRoot = Join-Path $PSScriptRoot '../rust/assets'
New-Item -ItemType Directory -Path $assetRoot -Force | Out-Null
$sizes = @(16, 20, 24, 32, 40, 48, 64, 128, 256)
$images = @()
foreach ($size in $sizes) {
    $bitmap = [Drawing.Bitmap]::new($size, $size)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = 'AntiAlias'
    $graphics.Clear([Drawing.Color]::Transparent)
    $graphics.ScaleTransform($size / 256.0, $size / 256.0)
    $shape = [Drawing.Drawing2D.GraphicsPath]::new()
    $shape.AddArc(8, 8, 88, 88, 180, 90)
    $shape.AddArc(160, 8, 88, 88, 270, 90)
    $shape.AddArc(160, 160, 88, 88, 0, 90)
    $shape.AddArc(8, 160, 88, 88, 90, 90)
    $shape.CloseFigure()
    $brush = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(255, 43, 111, 226))
    $graphics.FillPath($brush, $shape)
    $ring = [Drawing.Pen]::new([Drawing.Color]::FromArgb(70, 255, 255, 255), 4)
    $graphics.DrawEllipse($ring, 35, 35, 186, 186)
    $stroke = [Drawing.Pen]::new([Drawing.Color]::White, 19)
    $stroke.StartCap = 'Round'; $stroke.EndCap = 'Round'; $stroke.LineJoin = 'Round'
    $ribbon = [Drawing.Drawing2D.GraphicsPath]::new()
    $ribbon.AddBezier(170, 79, 139, 48, 83, 61, 84, 100)
    $ribbon.AddBezier(84, 100, 85, 129, 172, 120, 172, 157)
    $ribbon.AddBezier(172, 157, 172, 196, 112, 203, 82, 177)
    $graphics.DrawPath($stroke, $ribbon)
    $stream = [IO.MemoryStream]::new()
    $bitmap.Save($stream, [Drawing.Imaging.ImageFormat]::Png)
    $images += ,($stream.ToArray())
    if ($size -eq 256) { $bitmap.Save((Join-Path $assetRoot 'SlimBrowser.png'), [Drawing.Imaging.ImageFormat]::Png) }
    if ($size -eq 64) {
        $pixels = [Collections.Generic.List[byte]]::new()
        for ($y = 0; $y -lt 64; $y++) { for ($x = 0; $x -lt 64; $x++) { $c = $bitmap.GetPixel($x, $y); $pixels.AddRange([byte[]]@($c.R, $c.G, $c.B, $c.A)) } }
        [IO.File]::WriteAllBytes((Join-Path $assetRoot 'icon-64.rgba'), $pixels.ToArray())
    }
    $stream.Dispose(); $graphics.Dispose(); $bitmap.Dispose(); $shape.Dispose(); $ribbon.Dispose(); $brush.Dispose(); $ring.Dispose(); $stroke.Dispose()
}
$file = [IO.File]::Create((Join-Path $assetRoot 'SlimBrowser.ico'))
$writer = [IO.BinaryWriter]::new($file)
$writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i=0; $i -lt $sizes.Count; $i++) {
    $dimension = if ($sizes[$i] -eq 256) { 0 } else { $sizes[$i] }
    $writer.Write([byte]$dimension); $writer.Write([byte]$dimension); $writer.Write([byte]0); $writer.Write([byte]0)
    $writer.Write([uint16]1); $writer.Write([uint16]32); $writer.Write([uint32]$images[$i].Length); $writer.Write([uint32]$offset)
    $offset += $images[$i].Length
}
foreach ($bytes in $images) { $writer.Write([byte[]]$bytes) }
$writer.Dispose()
Write-Output "Created Slim Browser icon in $assetRoot"
