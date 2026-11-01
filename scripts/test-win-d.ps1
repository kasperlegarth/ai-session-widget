<#
.SYNOPSIS
    Presses Win+D twice (real key events) and reports, after each press, which
    app windows are minimized or above the desktop and where the widget sits.

.DESCRIPTION
    Checks the window layers of src-tauri/src/desktop.rs against Windows'
    "Show desktop". Start the widget first and choose the layer to test in its
    menu (Window > Always on top / Pin to desktop / both off).

    NOT read-only: it really shows the desktop and back, so your windows
    disappear for a few seconds. If the second Win+D fails, they stay away;
    bring them back from the taskbar.

    Expected, pinned to the desktop:
      after the 1st Win+D  no open window above the desktop, widget visible
      after the 2nd Win+D  all windows back, widget covers 0 open windows
    Expected, always on top:
      after the 1st Win+D  widget minimized like the other windows
      after the 2nd Win+D  all windows back, widget topmost=True

.PARAMETER WaitSeconds
    Pause after each Win+D, long enough for the widget's 2 s polls to run.
#>
param([int]$WaitSeconds = 4)

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class WD {
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern IntPtr GetTopWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetLong(IntPtr h, int i);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
"@

$process = Get-Process -Name 'AI Code Session Widget' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $process) { Write-Error "The widget is not running."; exit 1 }
$widgetPid = $process.Id

function Get-ClassName([IntPtr]$h) {
  $sb = New-Object System.Text.StringBuilder 128
  [void][WD]::GetClassName($h, $sb, 128)
  $sb.ToString()
}

# Walks the z-order from the top. "App windows": visible, titled, not tool
# windows, unowned or with their own taskbar button.
function Snapshot([string]$label) {
  $rows = @()
  $h = [WD]::GetTopWindow([IntPtr]::Zero)
  $pos = 0
  while ($h -ne [IntPtr]::Zero) {
    if ([WD]::IsWindowVisible($h)) {
      $ex = [WD]::GetLong($h, -20).ToInt64()
      $owner = [WD]::GetWindow($h, 4)
      $cls = Get-ClassName $h
      $txt = New-Object System.Text.StringBuilder 128; [void][WD]::GetWindowText($h, $txt, 128)
      $p = 0; [void][WD]::GetWindowThreadProcessId($h, [ref]$p)
      $isApp = (($ex -band 0x80) -eq 0) -and ($txt.Length -gt 0) -and (($owner -eq [IntPtr]::Zero) -or (($ex -band 0x40000) -ne 0))
      $isDesk = ($cls -eq 'Progman' -or $cls -eq 'WorkerW')
      if ($isApp -or $isDesk -or $p -eq $widgetPid) {
        $rows += [pscustomobject]@{ z = $pos; widget = ($p -eq $widgetPid -and $cls -eq 'Tauri Window'); iconic = [WD]::IsIconic($h); topmost = (($ex -band 8) -ne 0); cls = $cls }
      }
    }
    $pos++
    $h = [WD]::GetWindow($h, 2)
  }
  $apps = $rows | Where-Object { -not $_.widget -and $_.cls -ne 'Progman' -and $_.cls -ne 'WorkerW' }
  $desk = $rows | Where-Object { $_.cls -eq 'Progman' -or $_.cls -eq 'WorkerW' } | Select-Object -First 1
  $w = $rows | Where-Object { $_.widget } | Select-Object -First 1
  $aboveDesk = @($apps | Where-Object { -not $_.iconic -and $desk -and $_.z -lt $desk.z }).Count
  "--- $label (foreground: $(Get-ClassName ([WD]::GetForegroundWindow())))"
  "  app windows: $(@($apps).Count), minimized: $(@($apps | Where-Object iconic).Count), open above the desktop: $aboveDesk"
  if ($w) {
    $covered = @($apps | Where-Object { -not $_.iconic -and $_.z -gt $w.z -and $_.z -lt $desk.z }).Count
    "  widget: z=$($w.z) topmost=$($w.topmost) minimized=$($w.iconic); desktop ($($desk.cls)) z=$($desk.z); open windows the widget covers: $covered"
  } else {
    "  widget: not visible"
  }
}

function Send-WinD {
  [WD]::keybd_event(0x5B, 0, 0, [UIntPtr]::Zero)      # LWIN down
  [WD]::keybd_event(0x44, 0, 0, [UIntPtr]::Zero)      # D down
  [WD]::keybd_event(0x44, 0, 2, [UIntPtr]::Zero)      # D up
  [WD]::keybd_event(0x5B, 0, 2, [UIntPtr]::Zero)      # LWIN up
}

Snapshot "before"
Send-WinD; Start-Sleep -Seconds $WaitSeconds
Snapshot "after 1st Win+D (waited $WaitSeconds s)"
Send-WinD; Start-Sleep -Seconds $WaitSeconds
Snapshot "after 2nd Win+D (waited $WaitSeconds s)"
