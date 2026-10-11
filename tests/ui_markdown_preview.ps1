param([string]$Executable = (Join-Path $PSScriptRoot '..\target\release\rstpd.exe'))
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class MarkdownUi {
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window, int id);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int x, y; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect r);
    [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr h, ref Point p);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern IntPtr GetMenu(IntPtr h);
    [DllImport("user32.dll")] public static extern uint GetMenuState(IntPtr h, uint id, uint flags);
    [DllImport("user32.dll")] public static extern bool GetMenuItemRect(IntPtr h, IntPtr menu, uint index, out Rect r);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point p);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    delegate bool EnumWindow(IntPtr h, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumThreadWindows(uint thread, EnumWindow callback, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr h, EnumWindow callback, IntPtr data);
    public static IntPtr DialogButton(IntPtr dialog) {
        IntPtr result=IntPtr.Zero;
        EnumChildWindows(dialog,(h,d)=>{ var n=new System.Text.StringBuilder(64); GetClassName(h,n,64); if(n.ToString()=="Button") {result=h; return false;} return true; },IntPtr.Zero);
        return result;
    }
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, System.Text.StringBuilder name, int count);
    public static IntPtr OwnedWindow(IntPtr owner, string cls) {
        uint pid; var thread = GetWindowThreadProcessId(owner, out pid); IntPtr result = IntPtr.Zero;
        EnumThreadWindows(thread, (h, d) => {
            var name = new System.Text.StringBuilder(64); GetClassName(h, name, 64);
            if (name.ToString() == cls && IsWindowVisible(h)) { result = h; return false; } return true;
        }, IntPtr.Zero); return result;
    }
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
    public static string Text(IntPtr h) { var s = new System.Text.StringBuilder(4096); GetWindowText(h,s,4096); return s.ToString(); }
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] static extern IntPtr VirtualAllocEx(IntPtr p, IntPtr a, UIntPtr size, uint type, uint protection);
    [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr p, IntPtr a, byte[] b, UIntPtr size, out UIntPtr read);
    [DllImport("kernel32.dll")] static extern bool VirtualFreeEx(IntPtr p, IntPtr a, UIntPtr size, uint type);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr p);
    public static Rect TabRect(IntPtr tab, int index) {
        uint pid; GetWindowThreadProcessId(tab, out pid); var p = OpenProcess(0x38,false,pid);
        var a = VirtualAllocEx(p,IntPtr.Zero,(UIntPtr)16,0x3000,4);
        try {
            if (SendMessage(tab,0x130A,(IntPtr)index,a)==IntPtr.Zero) throw new Exception("Missing tab");
            var b=new byte[16]; UIntPtr read;
            if (!ReadProcessMemory(p,a,b,(UIntPtr)16,out read)) throw new Exception("Cannot read tab");
            return new Rect {left=BitConverter.ToInt32(b,0),top=BitConverter.ToInt32(b,4),right=BitConverter.ToInt32(b,8),bottom=BitConverter.ToInt32(b,12)};
        } finally { VirtualFreeEx(p,a,UIntPtr.Zero,0x8000); CloseHandle(p); }
    }
    [DllImport("user32.dll")] static extern bool OpenClipboard(IntPtr h);
    [DllImport("user32.dll")] static extern bool CloseClipboard();
    [DllImport("user32.dll")] static extern IntPtr GetClipboardData(uint format);
    [DllImport("kernel32.dll")] static extern IntPtr GlobalLock(IntPtr h);
    [DllImport("kernel32.dll")] static extern bool GlobalUnlock(IntPtr h);
    public static string Clipboard() {
        if (!OpenClipboard(IntPtr.Zero)) throw new Exception("Clipboard busy");
        try { var h=GetClipboardData(13); var p=GlobalLock(h); try { return Marshal.PtrToStringUni(p); } finally { GlobalUnlock(h); } }
        finally { CloseClipboard(); }
    }
}
'@
[void][MarkdownUi]::SetThreadDpiAwarenessContext([IntPtr](-4))
$root = Split-Path $PSScriptRoot -Parent
$workspace = Join-Path $root "target\markdown-smoke-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory $workspace | Out-Null
$process = $null
$originalCursor = [MarkdownUi+Point]::new()
[void][MarkdownUi]::GetCursorPos([ref]$originalCursor)
function Send($window, $message, $w = 0, $l = 0) {
    [MarkdownUi]::SendMessage($window, $message, [IntPtr]$w, [IntPtr]$l).ToInt64()
}
function PointParam($x, $y) { ($y -shl 16) -bor ($x -band 0xffff) }
function TabClick($index) {
    $r = [MarkdownUi]::TabRect($tabs, $index)
    $p = PointParam ($r.left + 12) ($r.top + 8)
    $null = Send $tabs 0x201 1 $p
    $null = Send $tabs 0x202 0 $p
    Start-Sleep -Milliseconds 350
}
function Context($index, $command, $choose = $false, $disabled = $false) {
    $r = [MarkdownUi]::TabRect($tabs, $index)
    $p = PointParam ($r.left + 12) ($r.top + 8)
    [void][MarkdownUi]::PostMessage($tabs, 0x205, [IntPtr]0, [IntPtr]$p)
    Start-Sleep -Milliseconds 250
    $popup = [MarkdownUi]::OwnedWindow($window, '#32768')
    if ($popup -eq [IntPtr]::Zero) { throw 'Tab context menu unavailable.' }
    $menu = [IntPtr](Send $popup 0x1e1)
    if ([MarkdownUi]::GetMenuState($menu, $command, 0) -eq [uint32]::MaxValue) { throw "Context action $command missing." }
    if ($disabled -and (([MarkdownUi]::GetMenuState($menu,$command,0) -band 3) -eq 0)) { throw 'Unnamed tab path action enabled.' }
    if ($choose) {
        $position = switch ($command) { 1055 {3} 1056 {4} 1057 {5} 1504 {6} 1506 {7} }
        $r = [MarkdownUi+Rect]::new()
        if (-not [MarkdownUi]::GetMenuItemRect([IntPtr]::Zero,$menu,$position,[ref]$r)) { throw 'Cannot locate popup action.' }
        [void][MarkdownUi]::SetCursorPos(($r.left+15),($r.top+8))
        Start-Sleep -Milliseconds 100
        [MarkdownUi]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
        [MarkdownUi]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
    } else {
        [void][MarkdownUi]::PostMessage($window,0x1f,[IntPtr]0,[IntPtr]0)
    }
    Start-Sleep -Milliseconds 300
    if ([MarkdownUi]::OwnedWindow($window,'#32768') -ne [IntPtr]::Zero) { throw 'Context menu did not close.' }
}
try {
    $process = Start-Process -FilePath $Executable -ArgumentList @('--session-dir', "`"$workspace`"", "`"$(Join-Path $root 'README.md')`"") -PassThru
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
        if ($process.HasExited) { throw 'Editor exited during startup.' }
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
    }
    $window = $process.MainWindowHandle
    $source = [MarkdownUi]::GetDlgItem($window, 101)
    $preview = [MarkdownUi]::GetDlgItem($window, 106)
    $tabs = [MarkdownUi]::GetDlgItem($window, 302)
    if ($source -eq [IntPtr]::Zero -or $preview -eq [IntPtr]::Zero) { throw 'Missing native controls.' }
    $previewMenuState = [MarkdownUi]::GetMenuState([MarkdownUi]::GetMenu($window),1055,0)
    if ($previewMenuState -eq [uint32]::MaxValue -or ($previewMenuState -band 3) -ne 0) { throw 'View menu Markdown preview action missing or disabled.' }
    $null = Send $window 0x111 1055
    Start-Sleep -Milliseconds 500
    if (-not [MarkdownUi]::IsWindowVisible($preview)) { throw 'Toggle did not show preview.' }
    if ((Send $preview 2006) -le 0) { throw 'Preview is empty.' }
    if ((Send $preview 2140) -ne 1) { throw 'Preview is editable.' }
    if ((Send $preview 2010) -ne 6) { throw 'Heading is not styled.' }
    $sourceLength = Send $source 2006
    $null = Send $preview 2004
    if ((Send $preview 2006) -le 0) { throw 'Read-only preview accepted a destructive edit.' }
    $null = Send $source 2004
    Start-Sleep -Milliseconds 500
    if ((Send $preview 2006) -ne 0) { throw 'Preview did not refresh after source edit.' }
    $null = Send $source 2176
    Start-Sleep -Milliseconds 500
    if ((Send $source 2006) -ne $sourceLength -or (Send $preview 2006) -le 0) { throw 'Undo did not refresh preview.' }
    $before = [MarkdownUi+Rect]::new()
    [void][MarkdownUi]::GetWindowRect($preview, [ref]$before)
    $point = [MarkdownUi+Point]::new()
    $point.x = $before.left - 5; $point.y = $before.top + 80
    [void][MarkdownUi]::ScreenToClient($window, [ref]$point)
    $null = Send $window 0x201 1 (PointParam $point.x $point.y)
    $null = Send $window 0x200 1 (PointParam ($point.x - 100) $point.y)
    $null = Send $window 0x202 0 (PointParam ($point.x - 100) $point.y)
    Start-Sleep -Milliseconds 300
    $after = [MarkdownUi+Rect]::new()
    [void][MarkdownUi]::GetWindowRect($preview, [ref]$after)
    if ($after.left -ge $before.left - 50) { throw 'Preview divider did not resize.' }
    $sourceAfter = [MarkdownUi+Rect]::new()
    [void][MarkdownUi]::GetWindowRect($source,[ref]$sourceAfter)
    $previewFraction = ($after.right - $after.left) / [double]($after.right - $sourceAfter.left)
    $null = Send $window 0x111 1006
    if (-not $process.WaitForExit(5000)) { throw 'Editor did not exit for recovery verification.' }
    $process = Start-Process -FilePath $Executable -ArgumentList @('--session-dir', "`"$workspace`"") -PassThru
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
        if ($process.HasExited) { throw 'Editor exited during recovery verification.' }
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
    }
    $window = $process.MainWindowHandle
    $source = [MarkdownUi]::GetDlgItem($window,101)
    $preview = [MarkdownUi]::GetDlgItem($window,106)
    $tabs = [MarkdownUi]::GetDlgItem($window,302)
    Start-Sleep -Milliseconds 500
    if (-not [MarkdownUi]::IsWindowVisible($preview)) { throw 'Restart discarded the document Markdown viewer preference.' }
    if ((Send $preview 2006) -le 0 -or (Send $preview 2010) -ne 6) { throw 'Recovered viewer did not render Markdown.' }
    $restored = [MarkdownUi+Rect]::new()
    [void][MarkdownUi]::GetWindowRect($preview,[ref]$restored)
    $sourceRestored = [MarkdownUi+Rect]::new()
    [void][MarkdownUi]::GetWindowRect($source,[ref]$sourceRestored)
    $restoredFraction = ($restored.right - $restored.left) / [double]($restored.right - $sourceRestored.left)
    if ([Math]::Abs($restoredFraction - $previewFraction) -gt 0.01) { throw 'Restart discarded the viewer divider position.' }
    $null = Send $window 0x111 1057
    Start-Sleep -Milliseconds 300
    if ([MarkdownUi]::Clipboard() -ne (Join-Path $root 'README.md')) { throw 'File Copy full path copied wrong text.' }
    foreach ($id in @(1056,1057)) {
        if ([MarkdownUi]::GetMenuState([MarkdownUi]::GetMenu($window), $id, 0) -eq [uint32]::MaxValue) { throw 'File path action missing.' }
        Context 0 $id
    }
    [void][MarkdownUi]::PostMessage($window, 0x111, [IntPtr]1056, [IntPtr]0)
    Start-Sleep -Milliseconds 300
    $dialog = [MarkdownUi]::OwnedWindow($window, '#32770')
    if ($dialog -eq [IntPtr]::Zero -or [MarkdownUi]::Text([MarkdownUi]::GetDlgItem($dialog,65535)) -notlike '*README.md*') { throw 'Full path dialog missing path.' }
    [void][MarkdownUi]::PostMessage([MarkdownUi]::DialogButton($dialog),0xf5,[IntPtr]0,[IntPtr]0)
    Start-Sleep -Milliseconds 400
    if ([MarkdownUi]::OwnedWindow($window,'#32770') -ne [IntPtr]::Zero) { throw "Full path dialog did not close: $([MarkdownUi]::Text($dialog)); OK=$([MarkdownUi]::GetDlgItem($dialog,1)); remaining=$([MarkdownUi]::Text([MarkdownUi]::OwnedWindow($window,'#32770')))" }
    $null = Send $window 0x111 1052
    $null = Send $window 0x111 1051
    $null = Send $window 0x111 1062
    Start-Sleep -Milliseconds 500
    if ((Send $preview 2010) -ne 6) { throw 'Theme change lost rendered styles.' }
    $null = Send $window 0x111 1001
    Start-Sleep -Milliseconds 500
    if ([MarkdownUi]::IsWindowVisible($preview)) { throw "New document inherited preview enablement: $([MarkdownUi]::Text($window)); tabs=$(Send $tabs 0x1304); selected=$(Send $tabs 0x130b)" }
    foreach ($id in @(1056,1057)) {
        if (([MarkdownUi]::GetMenuState([MarkdownUi]::GetMenu($window), $id, 0) -band 3) -eq 0) { throw 'Unnamed File path action enabled.' }
        Context 1 $id $false $true
    }
    $null = Send $preview 2013
    $null = Send $preview 2178
    Start-Sleep -Milliseconds 150
    Context 0 1057 $true
    if ([MarkdownUi]::Clipboard() -ne (Join-Path $root 'README.md')) { throw 'Clicked inactive tab copied wrong path.' }
    if ([MarkdownUi]::IsWindowVisible($preview)) { throw 'Path action enabled preview on unnamed document.' }
    Context 0 1056 $true
    $dialog = [MarkdownUi]::OwnedWindow($window,'#32770')
    if ($dialog -eq [IntPtr]::Zero -or [MarkdownUi]::Text([MarkdownUi]::GetDlgItem($dialog,65535)) -notlike '*README.md*') { throw 'Inactive clicked tab full path dialog targeted wrong document.' }
    [void][MarkdownUi]::PostMessage([MarkdownUi]::DialogButton($dialog),0xf5,[IntPtr]0,[IntPtr]0)
    Start-Sleep -Milliseconds 300
    Context 0 1055 $true
    if ([MarkdownUi]::Text($window) -notlike 'README.md*') { throw 'Context preview did not select clicked document.' }
    if ([MarkdownUi]::IsWindowVisible($preview)) { throw 'Clicked tab preview toggle targeted active unnamed tab instead.' }
    TabClick 1
    if ([MarkdownUi]::IsWindowVisible($preview)) { throw 'Unnamed tab acquired preview from context action.' }
    TabClick 0
    $null = Send $window 0x111 1055
    Start-Sleep -Milliseconds 300
    if (-not [MarkdownUi]::IsWindowVisible($preview)) { throw 'Returning did not restore preview.' }
    TabClick 1
    if ([MarkdownUi]::IsWindowVisible($preview)) { throw 'Unnamed tab inherited opted-in preview.' }
    TabClick 0
    if (-not [MarkdownUi]::IsWindowVisible($preview)) { throw 'Opted-in preview was not restored after tab switch.' }
    $null = Send $window 0x111 1055
    Start-Sleep -Milliseconds 500
    if ([MarkdownUi]::IsWindowVisible($preview)) { throw 'Toggle did not hide preview.' }
    $null = Send $window 0x111 1050
    Start-Sleep -Milliseconds 300
    $right = [MarkdownUi]::GetDlgItem($window,102)
    if (-not [MarkdownUi]::IsWindowVisible($right)) { throw 'Existing split unavailable.' }
    $null = Send $window 0x111 1055
    Start-Sleep -Milliseconds 300
    if (-not [MarkdownUi]::IsWindowVisible($right) -or -not [MarkdownUi]::IsWindowVisible($preview)) { throw 'Preview lost existing split.' }
    $null = Send $window 0x111 1055
    Start-Sleep -Milliseconds 300
    if (-not [MarkdownUi]::IsWindowVisible($right)) { throw 'Disabling preview lost existing split.' }
    $null = Send $window 0x111 1055
    $null = Send $window 0x111 1506
    Start-Sleep -Milliseconds 300
    if (-not [MarkdownUi]::IsWindowVisible($preview) -or (Send $source 2357) -ne (Send $right 2357)) { throw 'Clone lost document-scoped preview.' }
    $null = Send $window 0x111 1005
    Start-Sleep -Milliseconds 300
    if ([MarkdownUi]::IsWindowVisible($preview)) { throw 'Closing clone leaked preview onto its unnamed sibling.' }
    $tabs = [MarkdownUi]::GetDlgItem($window,304)
    TabClick 0
    if (-not [MarkdownUi]::IsWindowVisible($preview)) { throw 'Closing clone discarded original preview preference.' }
    $null = Send $window 0x111 1504
    Start-Sleep -Milliseconds 300
    if (-not [MarkdownUi]::IsWindowVisible($preview)) { throw 'Moving document discarded preview preference.' }
    $null = Send $window 0x111 1005
    Start-Sleep -Milliseconds 300
    if ([MarkdownUi]::IsWindowVisible($preview)) { throw 'Closing opted-in document leaked preview state.' }
    [void][MarkdownUi]::PostMessage($window, 0x111, [IntPtr]1150, [IntPtr]0)
    Start-Sleep -Milliseconds 300
    $dialog = [MarkdownUi]::OwnedWindow($window, '#32770')
    if ($dialog -eq [IntPtr]::Zero -or [MarkdownUi]::Text([MarkdownUi]::GetDlgItem($dialog,65535)) -notlike 'rstpd 1.4.4*') { throw 'Release About dialog does not report version 1.4.4.' }
    [void][MarkdownUi]::PostMessage([MarkdownUi]::DialogButton($dialog),0xf5,[IntPtr]0,[IntPtr]0)
    Start-Sleep -Milliseconds 300
    Write-Host 'Markdown UI smoke passed: version 1.4.4, native rendering, editing/undo, divider drag, session restart and divider recovery, per-document isolation/restore, clone/move/close, clicked-tab preview/path actions, File copy/dialog, unnamed disablement, theme and split/map/wrap.'
} finally {
    [void][MarkdownUi]::SetCursorPos($originalCursor.x,$originalCursor.y)
    if ($process -and -not $process.HasExited) {
        $null = $process.CloseMainWindow()
        if (-not $process.WaitForExit(5000)) {
            Stop-Process -Id $process.Id
            $process.WaitForExit()
        }
    }
    Remove-Item -LiteralPath $workspace -Recurse -Force
}
