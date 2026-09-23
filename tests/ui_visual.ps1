param(
    [string]$Executable = (Join-Path $PSScriptRoot '..\target\release\rstpd.exe'),
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\target\ui-visual'),
    [switch]$NativeCapture,
    [switch]$GdiDiagnostic
)
$ErrorActionPreference = 'Stop'
$Executable = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Executable)
$OutputDirectory = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class VisualUi {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int x, y; }
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h, int id);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int height, uint flags);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref Point p);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool RedrawWindow(IntPtr h, IntPtr rect, IntPtr region, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint from, uint to, bool attach);
    [DllImport("user32.dll")] static extern IntPtr SetFocus(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr SetActiveWindow(IntPtr h);
    public static void Activate(IntPtr window) {
        uint pid;
        uint target = GetWindowThreadProcessId(window, out pid), current = GetCurrentThreadId();
        if (!AttachThreadInput(current, target, true)) throw new Exception("Cannot attach owned fixture input queue");
        try { SetActiveWindow(window); SetForegroundWindow(window); SetFocus(GetDlgItem(window,102)); }
        finally { AttachThreadInput(current,target,false); }
    }
}
'@
[void][VisualUi]::SetThreadDpiAwarenessContext([IntPtr](-4))
$output = [IO.Path]::GetFullPath($OutputDirectory)
[void](New-Item -ItemType Directory -Force $output)
$directory = Join-Path $output ("visual-session-" + [Guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory $directory)
$leftFile = Join-Path $directory 'LEFT-original.txt'
$rightFile = Join-Path $directory 'RIGHT-moved.txt'
[IO.File]::WriteAllText($leftFile, "LEFT GROUP - ORIGINAL DOCUMENT`r`n`r`nThis tab belongs only to the LEFT pane.`r`nIts selection remains independent.`r`n`r`nAlpha: 111`r`nBravo: 222`r`n")
[IO.File]::WriteAllText($rightFile, "RIGHT GROUP - MOVED DOCUMENT`r`n`r`nOpen in split MOVED this document and tab.`r`nOnly Clone explicitly shares a document.`r`n`r`nCharlie: 333`r`nDelta: 444`r`n")
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = [IO.Path]::GetFullPath($Executable)
$start.UseShellExecute = $false
foreach ($arg in @('--session-dir', $directory, $leftFile, $rightFile)) { $start.ArgumentList.Add($arg) }
$app = [Diagnostics.Process]::Start($start)
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $app.Refresh()
        if ($app.HasExited) { throw 'Owned visual fixture exited' }
        if ($app.MainWindowHandle -ne 0) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    $window = $app.MainWindowHandle
    if ($window -eq 0) { throw 'Owned visual window unavailable' }
    [void][VisualUi]::SetWindowPos($window, [IntPtr](-1), 30, 30, 1180, 760, 0)
    [void][VisualUi]::SetForegroundWindow($window)
    [void][VisualUi]::PostMessage($window, 0x111, [IntPtr]1062, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 400
    [void][VisualUi]::PostMessage($window, 0x111, [IntPtr]1504, [IntPtr]::Zero)
    Start-Sleep -Seconds 1
    [void][VisualUi]::RedrawWindow($window, [IntPtr]::Zero, [IntPtr]::Zero, 0x185)
    [void][VisualUi]::DwmFlush()
    Start-Sleep -Seconds 1
    [VisualUi]::Activate($window)
    Start-Sleep -Milliseconds 200
    if ($GdiDiagnostic) {
        foreach ($id in @(101,102)) {
            [void][VisualUi]::SendMessage([VisualUi]::GetDlgItem($window,$id),2630,[IntPtr]::Zero,[IntPtr]::Zero)
        }
        [void][VisualUi]::RedrawWindow($window, [IntPtr]::Zero, [IntPtr]::Zero, 0x185)
        Start-Sleep -Milliseconds 300
    }
    Write-Output "Owned HWND=$window foreground=$([VisualUi]::GetForegroundWindow()) session=$([Diagnostics.Process]::GetCurrentProcess().SessionId)"
    $wr = [VisualUi+Rect]::new()
    [void][VisualUi]::GetWindowRect($window, [ref]$wr)
    foreach ($xy in $(if ($NativeCapture) { @() } else { @(@(20,20), @(600,200), @(1150,200), @(600,700)) })) {
        $point = [VisualUi+Point]::new()
        $point.x = $wr.left + $xy[0]; $point.y = $wr.top + $xy[1]
        if ([VisualUi]::GetAncestor([VisualUi]::WindowFromPoint($point), 2) -ne $window) {
            throw 'Owned screenshot area is obscured or desktop is unavailable'
        }
    }
    $bitmap = [Drawing.Bitmap]::new($wr.right-$wr.left, $wr.bottom-$wr.top)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        if ($NativeCapture) {
            $dc = $graphics.GetHdc()
            try {
                if (![VisualUi]::PrintWindow($window, $dc, 0)) { throw 'PrintWindow capture failed' }
            } finally { $graphics.ReleaseHdc($dc) }
        } else {
            $graphics.CopyFromScreen($wr.left, $wr.top, 0, 0, $bitmap.Size)
        }
        $bitmap.Save((Join-Path $output 'visual-capture-diagnostic.png'), [Drawing.Imaging.ImageFormat]::Png)
        foreach ($id in @(101, 102)) {
            $editor = [VisualUi]::GetDlgItem($window, $id)
            $r = [VisualUi+Rect]::new()
            [void][VisualUi]::GetWindowRect($editor, [ref]$r)
            $tab = [VisualUi]::GetDlgItem($window, $(if ($id -eq 101) {302} else {304}))
            if ([VisualUi]::SendMessage($tab,0x1304,[IntPtr]::Zero,[IntPtr]::Zero).ToInt64() -ne 1) { throw 'Each pane must own exactly one fixture tab' }
            $strip = $bitmap.GetPixel($r.left-$wr.left+100,$r.top-$wr.top-2)
            if ($strip.ToArgb() -ne [Drawing.Color]::FromArgb(35,38,44).ToArgb()) {
                throw 'Neither pane may have a pane-wide highlight'
            }
            foreach ($offset in @(70, 200, 350)) {
                $pixel = $bitmap.GetPixel($r.left-$wr.left+100, $r.top-$wr.top+$offset)
                if ($pixel.ToArgb() -ne [Drawing.Color]::FromArgb(27,29,34).ToArgb()) {
                    throw "Editor $id desktop background at $offset is $($pixel.Name), not dark editor background"
                }
            }
            $ink = 0
            for ($x = $r.left-$wr.left+45; $x -lt $r.left-$wr.left+350; $x++) {
                for ($y = $r.top-$wr.top+1; $y -lt $r.top-$wr.top+20; $y++) {
                    $color = $bitmap.GetPixel($x,$y)
                    if ($color.R -gt 120 -and $color.G -gt 120 -and $color.B -gt 120) { $ink++ }
                }
            }
            if ($ink -lt 50) { throw "Editor $id has no rendered first-line text ($ink bright pixels)" }
            if (!$GdiDiagnostic -and [VisualUi]::SendMessage($editor, 2631, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64() -ne 1) {
                throw 'Capture failed to restore normal DirectWrite rendering'
            }
            Write-Output "PASS editor $id background and rendered text ($ink bright pixels), GdiDiagnostic=$GdiDiagnostic"
        }
        $origin = [VisualUi+Point]::new()
        [void][VisualUi]::ClientToScreen($window, [ref]$origin)
        $rule = $bitmap.GetPixel(100, $origin.y-$wr.top-1)
        if ($rule.ToArgb() -ne [Drawing.Color]::FromArgb(35,38,44).ToArgb()) { throw 'Desktop menu separator is not dark' }
        $path = Join-Path $output $(if ($GdiDiagnostic) { 'groups-gdi-diagnostic.png' } elseif ($NativeCapture) { 'groups-native.png' } else { 'groups-desktop.png' })
        $bitmap.Save($path, [Drawing.Imaging.ImageFormat]::Png)
        Write-Output "Visual evidence (NativeCapture=$NativeCapture): $path"
        $dc = $graphics.GetHdc()
        try { [void][VisualUi]::PrintWindow($window, $dc, 2) }
        finally { $graphics.ReleaseHdc($dc) }
        $bitmap.Save((Join-Path $output 'groups-printwindow-diagnostic.png'), [Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    if (!$app.CloseMainWindow() -or !$app.WaitForExit(10000)) { throw 'Owned visual fixture did not close normally' }
} finally {
    if (!$app.HasExited) { Stop-Process -Id $app.Id; $app.WaitForExit() }
    Remove-Item -LiteralPath $directory -Recurse -Force
}
