Add-Type -AssemblyName System.Drawing
$root = "C:/Users/tsuser/.zcode/workspace/default/Advoco/src-tauri/icons"
$b = New-Object System.Drawing.Bitmap 256,256
$g = [System.Drawing.Graphics]::FromImage($b)
$g.SmoothingMode = "AntiAlias"
$bg = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(38,32,54))
$g.FillRectangle($bg,0,0,256,256)
$acc = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(232,181,96))
$g.FillEllipse($acc,96,120,64,64)
$g.FillEllipse($acc,64,84,28,28)
$g.FillEllipse($acc,98,66,28,28)
$g.FillEllipse($acc,134,66,28,28)
$g.FillEllipse($acc,164,84,28,28)
$font = New-Object System.Drawing.Font("Segoe UI",40,[System.Drawing.FontStyle]::Bold)
$g.DrawString("A",$font,[System.Drawing.Brushes]::White,14,10)
$g.Dispose()
$b.Save("$root/icon.png",[System.Drawing.Imaging.ImageFormat]::Png)
$png = [System.IO.File]::ReadAllBytes("$root/icon.png")
$ms = New-Object System.IO.MemoryStream
$ms.Write([byte[]](0,0,1,0,1,0),0,6)
# dir entry: width=0 (256), height=0 (256), colors=0, reserved=0
$ms.Write([byte[]](0,0,0,0),0,4)
# planes=1, bitcount=32 (4 bytes)
$ms.Write([byte[]](1,0,32,0),0,4)
$sizes = [System.BitConverter]::GetBytes([uint32]$png.Length); $ms.Write($sizes,0,4)
$off = [System.BitConverter]::GetBytes([uint32]22); $ms.Write($off,0,4)
$ms.Write($png,0,$png.Length)
[System.IO.File]::WriteAllBytes("$root/icon.ico",$ms.ToArray())
Write-Output "icon written"
