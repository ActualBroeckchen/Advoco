Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$p = Get-Process advoco | Select-Object -First 1
if (-not $p) { Write-Output "no advoco process"; exit 1 }
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
[Win32]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 600
$r = New-Object Win32+RECT
[Win32]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
$w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
$b = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($b)
$g.CopyFromScreen($r.Left, $r.Top, 0, 0, (New-Object System.Drawing.Size $w, $h))
$out = "C:\Users\tsuser\.zcode\workspace\default\Advoco\src-tauri\target\advoco-window.png"
$ms = New-Object System.IO.MemoryStream
$b.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
[System.IO.File]::WriteAllBytes($out, $ms.ToArray())
Write-Output "saved $out ($w x $h)"
