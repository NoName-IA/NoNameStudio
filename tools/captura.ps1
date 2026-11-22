# Abre NoName Studio, espera, captura su ventana a PNG y (opcional) la cierra.
param([string]$Exe, [string]$Out, [int]$Wait = 4, [string]$Keys = "", [string]$AppArgs = "", [switch]$Close)
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices;
public class W { [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
public struct RECT { public int L, T, R, B; } }
"@
$p = Get-Process NoNameStudio -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) { if ($AppArgs) { $p = Start-Process $Exe -ArgumentList $AppArgs -PassThru } else { $p = Start-Process $Exe -PassThru }; Start-Sleep $Wait }
$p.Refresh(); $h = $p.MainWindowHandle
[W]::ShowWindow($h, 5) | Out-Null; [W]::SetForegroundWindow($h) | Out-Null; Start-Sleep -Milliseconds 500
foreach ($k in ($Keys -split ',')) { if ($k) { [System.Windows.Forms.SendKeys]::SendWait($k); Start-Sleep -Milliseconds 900 } }
$r = New-Object W+RECT; [W]::GetWindowRect($h, [ref]$r) | Out-Null
$bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
$g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
"captura {0}x{1} -> {2}" -f ($r.R - $r.L), ($r.B - $r.T), $Out
if ($Close) { $p.CloseMainWindow() | Out-Null }
