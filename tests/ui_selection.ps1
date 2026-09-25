param(
    [string]$Executable = (Join-Path $PSScriptRoot '..\target\release\rstpd.exe'),
    [string]$ScreenshotPath,
    [string]$ChromeScreenshotDirectory
)
$ErrorActionPreference = 'Stop'
$Executable = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Executable)
if ($ScreenshotPath) {
    $ScreenshotPath = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ScreenshotPath)
}
if ($ChromeScreenshotDirectory) {
    $ChromeScreenshotDirectory = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ChromeScreenshotDirectory)
}
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class SelectionUi {
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h, int id);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint thread, ref Gui info);
    delegate bool EnumWindow(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] static extern bool EnumThreadWindows(uint thread, EnumWindow callback, IntPtr data);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, System.Text.StringBuilder name, int count);
    [DllImport("user32.dll")] public static extern uint GetMenuState(IntPtr menu, uint item, uint flags);
    public static IntPtr Popup(IntPtr owner) {
        uint pid;
        uint thread = GetWindowThreadProcessId(owner, out pid);
        IntPtr result = IntPtr.Zero;
        EnumThreadWindows(thread, (window, data) => {
            var name = new System.Text.StringBuilder(64);
            GetClassName(window, name, name.Capacity);
            if (name.ToString() == "#32768") { result = window; return false; }
            return true;
        }, IntPtr.Zero);
        if (result == IntPtr.Zero) throw new Exception("Owned context menu unavailable");
        return result;
    }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window, ref Point point);
    [DllImport("user32.dll")] public static extern bool GetMenuBarInfo(IntPtr window, int objectId, int item, ref MenuBar info);
    [DllImport("user32.dll")] public static extern IntPtr GetWindowDC(IntPtr window);
    [DllImport("user32.dll")] public static extern int ReleaseDC(IntPtr window, IntPtr dc);
    [DllImport("gdi32.dll")] public static extern uint GetPixel(IntPtr dc, int x, int y);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int x, y; }
    [StructLayout(LayoutKind.Sequential)] public struct MenuBar {
        public uint size; public Rect rect; public IntPtr menu, menuWindow; public uint flags;
    }
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint from, uint to, bool attach);
    [DllImport("user32.dll")] static extern bool GetKeyboardState(byte[] state);
    [DllImport("user32.dll")] static extern bool SetKeyboardState(byte[] state);
    [DllImport("user32.dll")] static extern IntPtr SetActiveWindow(IntPtr window);
    [DllImport("user32.dll")] static extern IntPtr SetFocus(IntPtr window);
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll")] static extern IntPtr VirtualAllocEx(IntPtr process, IntPtr address, UIntPtr size, uint type, uint protect);
    [DllImport("kernel32.dll")] static extern bool VirtualFreeEx(IntPtr process, IntPtr address, UIntPtr size, uint type);
    [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr process, IntPtr address, byte[] bytes, UIntPtr size, out UIntPtr count);
    [DllImport("kernel32.dll")] static extern bool WriteProcessMemory(IntPtr process, IntPtr address, byte[] bytes, UIntPtr size, out UIntPtr count);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    public static byte[] TabData(IntPtr tab, int index, bool text) {
        uint pid;
        GetWindowThreadProcessId(tab, out pid);
        IntPtr process = OpenProcess(0x38, false, pid);
        if (process == IntPtr.Zero) throw new Exception("Cannot inspect isolated tab control");
        IntPtr remote = VirtualAllocEx(process, IntPtr.Zero, (UIntPtr)1024, 0x3000, 4);
        if (remote == IntPtr.Zero) { CloseHandle(process); throw new Exception("Cannot allocate tab buffer"); }
        try {
            UIntPtr count;
            var bytes = new byte[1024];
            if (text) {
                BitConverter.GetBytes(1).CopyTo(bytes, 0);
                BitConverter.GetBytes(remote.ToInt64() + 64).CopyTo(bytes, 16);
                BitConverter.GetBytes(400).CopyTo(bytes, 24);
                if (!WriteProcessMemory(process, remote, bytes, (UIntPtr)1024, out count)) throw new Exception("Cannot initialize tab buffer");
            }
            if (SendMessage(tab, text ? 0x133Cu : 0x130Au, (IntPtr)index, remote) == IntPtr.Zero)
                throw new Exception("Cannot read native tab");
            if (!ReadProcessMemory(process, remote, bytes, (UIntPtr)1024, out count)) throw new Exception("Cannot inspect tab result");
            return bytes;
        } finally { VirtualFreeEx(process, remote, UIntPtr.Zero, 0x8000); CloseHandle(process); }
    }
    public static string TabLabel(IntPtr tab, int index) {
        return System.Text.Encoding.Unicode.GetString(TabData(tab, index, true), 64, 800).Split('\0')[0];
    }
    public static void FocusEditor(IntPtr window, IntPtr editor) {
        uint pid;
        uint target = GetWindowThreadProcessId(window, out pid);
        uint current = GetCurrentThreadId();
        if (!AttachThreadInput(current, target, true)) throw new Exception("Cannot focus isolated editor");
        try { SetActiveWindow(window); SetFocus(editor); }
        finally { AttachThreadInput(current, target, false); }
    }
    public static void Key(IntPtr window, int key, bool control, bool shift) {
        uint pid;
        uint target = GetWindowThreadProcessId(window, out pid);
        uint current = GetCurrentThreadId();
        if (!AttachThreadInput(current, target, true)) throw new Exception("Cannot attach isolated input queue");
        var previous = new byte[256];
        GetKeyboardState(previous);
        try {
            var state = new byte[256];
            if (control) state[0x11] = 0x80;
            if (shift) state[0x10] = 0x80;
            if (!SetKeyboardState(state)) throw new Exception("Cannot set isolated keyboard state");
            if (!PostMessage(window, 0x100, (IntPtr)key, IntPtr.Zero)) throw new Exception("Cannot post isolated key");
            System.Threading.Thread.Sleep(250);
            PostMessage(window, 0x101, (IntPtr)key, IntPtr.Zero);
            System.Threading.Thread.Sleep(100);
        } finally { SetKeyboardState(previous); AttachThreadInput(current, target, false); }
    }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Gui {
        public uint size, flags;
        public IntPtr active, focus, capture, menuOwner, moveSize, caret;
        public Rect caretRect;
    }
}
'@
function Post([IntPtr]$Handle, [uint32]$Message, [long]$W = 0, [long]$L = 0) {
    if (![SelectionUi]::PostMessage($Handle, $Message, [IntPtr]$W, [IntPtr]$L)) { throw 'PostMessage failed' }
    Start-Sleep -Milliseconds 150
}
function Send([IntPtr]$Handle, [uint32]$Message, [long]$W = 0, [long]$L = 0) {
    [SelectionUi]::SendMessage($Handle, $Message, [IntPtr]$W, [IntPtr]$L).ToInt64()
}
function Click([IntPtr]$Handle, [int]$X = 90, [int]$Y = 40) {
    Post $Handle 0x201 1 (($Y -shl 16) -bor $X)
    Post $Handle 0x202 0 (($Y -shl 16) -bor $X)
}
function Keys([string]$Text) {
    switch ($Text) {
        '^n' { [SelectionUi]::Key($window, 0x4E, $true, $false) }
        '^{TAB}' { [SelectionUi]::Key($window, 0x09, $true, $false) }
        '^+{TAB}' { [SelectionUi]::Key($window, 0x09, $true, $true) }
        '{F6}' { [SelectionUi]::Key($window, 0x75, $false, $false) }
        'dd' {
            $info = [SelectionUi+Gui]::new()
            $info.size = [Runtime.InteropServices.Marshal]::SizeOf($info)
            [uint32]$processId = 0
            $thread = [SelectionUi]::GetWindowThreadProcessId($window, [ref]$processId)
            if (![SelectionUi]::GetGUIThreadInfo($thread, [ref]$info) -or $info.focus -notin @($left, $right)) {
                throw 'Refusing to type outside the isolated editors'
            }
            Post $info.focus 0x102 100
            Post $info.focus 0x102 100
        }
        default { throw 'Unsupported isolated test key' }
    }
    Start-Sleep -Milliseconds 250
}
function SelectTab([int]$Index, [int]$Pane = 0) {
    $tab = @($tabs, $rightTabs)[$Pane]
    $bytes = [SelectionUi]::TabData($tab, $Index, $false)
    $x = [int](([BitConverter]::ToInt32($bytes, 0) + [BitConverter]::ToInt32($bytes, 8)) / 2)
    $y = [int](([BitConverter]::ToInt32($bytes, 4) + [BitConverter]::ToInt32($bytes, 12)) / 2)
    Click $tab $x $y
}
function CheckChrome {
        [void](New-Item -ItemType Directory -Force $ChromeScreenshotDirectory)
        foreach ($theme in @('dark', 'light', 'dark')) {
            Post $window 0x111 $(if ($theme -eq 'dark') { 1062 } else { 1061 })
            $origin = [SelectionUi+Point]::new()
            [void][SelectionUi]::ClientToScreen($window, [ref]$origin)
            $tr = [SelectionUi+Rect]::new()
            $wr = [SelectionUi+Rect]::new()
            [void][SelectionUi]::GetWindowRect($tabs, [ref]$tr)
            [void][SelectionUi]::GetWindowRect($window, [ref]$wr)
            $dpi = [SelectionUi]::GetDpiForWindow($window)
            if (($tr.top - $origin.y) -ne [int][Math]::Floor(46 * $dpi / 96)) {
                throw 'Toolbar/tab spacing did not follow DPI layout'
            }
            $bitmap = [Drawing.Bitmap]::new($wr.right - $wr.left, $wr.bottom - $wr.top)
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            $dc = $graphics.GetHdc()
            try { if (![SelectionUi]::PrintWindow($window, $dc, 2)) { throw 'Chrome screenshot failed' } }
            finally { $graphics.ReleaseHdc($dc) }
            try {
                $panel = if ($theme -eq 'dark') { [Drawing.Color]::FromArgb(35, 38, 44) } else { [Drawing.Color]::FromArgb(244, 246, 249) }
                for ($y = $origin.y + [int][Math]::Floor(38 * $dpi / 96); $y -lt $tr.top; $y++) {
                    if ($bitmap.GetPixel(100, $y - $wr.top).ToArgb() -ne $panel.ToArgb()) {
                        throw "$theme toolbar/tab separation must be plain panel spacing, without extra rules"
                    }
                }
                $bar = [SelectionUi+MenuBar]::new()
                $bar.size = [Runtime.InteropServices.Marshal]::SizeOf($bar)
                if (![SelectionUi]::GetMenuBarInfo($window, -3, 0, [ref]$bar)) { throw 'Native menu bar unavailable' }
                $rule = $bitmap.GetPixel(100, $origin.y - $wr.top - 1)
                if ($theme -eq 'dark' -and $rule.ToArgb() -ne $panel.ToArgb()) {
                    throw "Dark native menu separator remains bright: $($rule.Name)"
                }
                Write-Output "$theme native menu bottom=$($bar.rect.bottom), client top=$($origin.y), native bottom pixel=$($rule.Name)"
                $bitmap.Save((Join-Path ([IO.Path]::GetFullPath($ChromeScreenshotDirectory)) "$theme.png"), [Drawing.Imaging.ImageFormat]::Png)
            } finally { $graphics.Dispose(); $bitmap.Dispose() }
            # Open the existing native File menu with its mnemonic, then run New.
            Click $left
            $before = Send $tabs 0x1304
            Post $window 0x112 0xF100 102
            $info = [SelectionUi+Gui]::new()
            $info.size = [Runtime.InteropServices.Marshal]::SizeOf($info)
            [uint32]$processId = 0
            $thread = [SelectionUi]::GetWindowThreadProcessId($window, [ref]$processId)
            [void][SelectionUi]::GetGUIThreadInfo($thread, [ref]$info)
            if (($info.flags -band 4) -eq 0) { throw "$theme native menu failed to enter its menu loop" }
            Post $window 0x102 110
            if ((Send $tabs 0x1304) -ne $before + 1) { throw "$theme File > New mnemonic did not execute" }
            if ($theme -eq 'dark') {
                foreach ($stage in @('menu dismissal', 'deactivation', 'activation', 'resize', 'nonclient repaint')) {
                    switch ($stage) {
                        'deactivation' { [void](Send $window 0x86 0) }
                        'activation' { [void](Send $window 0x86 1) }
                        'resize' { [void][SelectionUi]::SetWindowPos($window, [IntPtr]::Zero, 0, 0, $wr.right-$wr.left-20, $wr.bottom-$wr.top, 6) }
                        'nonclient repaint' { [void](Send $window 0x85 1) }
                    }
                    Start-Sleep -Milliseconds 150
                    $live = [SelectionUi]::GetWindowDC($window)
                    try {
                        $pixel = [SelectionUi]::GetPixel($live, 100, $origin.y-$wr.top-1)
                        if ($pixel -ne 0x2c2623) { throw "Live separator after $stage is $pixel instead of dark panel" }
                    } finally { [void][SelectionUi]::ReleaseDC($window, $live) }
                    Write-Output "PASS dark live native separator after $stage"
                }
            }
            Write-Output "PASS $theme native menu keyboard command and unbordered toolbar/tab gap"
        }
}
function AssertState([int]$Pane, [int]$Tab, [long]$LeftDocument, [long]$RightDocument, [string]$Stage) {
    $info = [SelectionUi+Gui]::new()
    $info.size = [Runtime.InteropServices.Marshal]::SizeOf($info)
    [uint32]$processId = 0
    $thread = [SelectionUi]::GetWindowThreadProcessId($window, [ref]$processId)
    if (![SelectionUi]::GetGUIThreadInfo($thread, [ref]$info)) { throw 'Cannot read GUI focus' }
    if ($info.focus -ne @($left, $right)[$Pane]) { throw "${Stage}: wrong native keyboard focus" }
    if ((Send @($tabs, $rightTabs)[$Pane] 0x130B) -ne $Tab) { throw "${Stage}: wrong selected tab" }
    if ((Send $left 2357) -ne $LeftDocument -or (Send $right 2357) -ne $RightDocument) {
        throw "${Stage}: tab selection replaced a visible document"
    }
    $wr = [SelectionUi+Rect]::new()
    [void][SelectionUi]::GetWindowRect($window, [ref]$wr)
    $bitmap = [Drawing.Bitmap]::new($wr.right - $wr.left, $wr.bottom - $wr.top)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try {
        if (![SelectionUi]::PrintWindow($window, $dc, 2)) { throw 'PrintWindow failed' }
    } finally { $graphics.ReleaseHdc($dc) }
    try {
        $colors = foreach ($editor in @($left, $right)) {
            $r = [SelectionUi+Rect]::new()
            [void][SelectionUi]::GetWindowRect($editor, [ref]$r)
            $bitmap.GetPixel($r.left - $wr.left + 30, $r.top - $wr.top - 2).ToArgb()
        }
        if ($colors[0] -ne [Drawing.Color]::FromArgb(35,38,44).ToArgb() -or $colors[1] -ne [Drawing.Color]::FromArgb(35,38,44).ToArgb()) {
            throw "${Stage}: unexpected pane-wide highlight"
        }
        $activeTabs = @($tabs, $rightTabs)[$Pane]
        $tr = [SelectionUi+Rect]::new()
        [void][SelectionUi]::GetWindowRect($activeTabs, [ref]$tr)
        $tabRect = [SelectionUi]::TabData($activeTabs, $Tab, $false)
        $underline = $bitmap.GetPixel($tr.left-$wr.left+[BitConverter]::ToInt32($tabRect,0)+20,
            $tr.top-$wr.top+[BitConverter]::ToInt32($tabRect,12)-1)
        if ($underline.ToArgb() -ne [Drawing.Color]::FromArgb(113,177,255).ToArgb()) {
            throw "${Stage}: selected tab underline was lost"
        }
        if ($ScreenshotPath -and $Stage -eq 'independent pane groups with right focus') {
            foreach ($editor in @($left, $right)) {
                $r = [SelectionUi+Rect]::new()
                [void][SelectionUi]::GetWindowRect($editor, [ref]$r)
                if ($bitmap.GetPixel($r.left-$wr.left+100, $r.top-$wr.top+70).ToArgb() -ne [Drawing.Color]::FromArgb(27,29,34).ToArgb()) {
                    throw 'Screenshot rejected: PrintWindow did not render a dark editor. Run ui_visual.ps1 on an available desktop.'
                }
            }
            $bitmap.Save([IO.Path]::GetFullPath($ScreenshotPath), [Drawing.Imaging.ImageFormat]::Png)
        }
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
    Write-Output "PASS $Stage"
}
[void][SelectionUi]::SetThreadDpiAwarenessContext([IntPtr](-4))
$directory = Join-Path $PSScriptRoot ("..\target\selection-check-" + [Guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory $directory)
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = [IO.Path]::GetFullPath($Executable)
$start.UseShellExecute = $false
$start.ArgumentList.Add('--session-dir')
$start.ArgumentList.Add([IO.Path]::GetFullPath($directory))
$app = [Diagnostics.Process]::Start($start)
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        if ($app.HasExited) { throw 'Isolated test app exited' }
        $app.Refresh()
        if ($app.MainWindowHandle -ne 0) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    $window = $app.MainWindowHandle
    if ($window -eq 0) { throw 'Isolated test window did not appear' }
    $left = [SelectionUi]::GetDlgItem($window, 101)
    $right = [SelectionUi]::GetDlgItem($window, 102)
    $tabs = [SelectionUi]::GetDlgItem($window, 302)
    $rightTabs = [SelectionUi]::GetDlgItem($window, 306)
    [SelectionUi]::FocusEditor($window, $left)
    Post $window 0x111 1062
    Click $left
    Keys 'dd'
    Keys '^n'
    Keys 'dd'
    $second = Send $left 2357
    # Splitting moves the selected document and tab, not a shared view.
    Post $window 0x111 1050
    $first = Send $left 2357
    if ($first -eq $second -or (Send $left 2006) -ne 2 -or (Send $right 2006) -ne 2) {
        throw "Fixture must show two different dirty documents containing dd: handles=$first,$second lengths=$(Send $left 2006),$(Send $right 2006)"
    }
    foreach ($editor in @($left, $right)) {
        if ((Send $editor 2007 0) -ne 100 -or (Send $editor 2007 1) -ne 100) {
            throw 'Fixture text must be exactly dd in each pane'
        }
    }
    if ((Send $tabs 0x1304) -ne 1 -or (Send $rightTabs 0x1304) -ne 1) { throw 'Move must produce separate group tab bars' }
    AssertState 1 0 $first $second 'independent pane groups with right focus'
    [void](Send $left 2160 0 0)
    [void](Send $right 2160 2 2)
    SelectTab 0 1
    AssertState 1 0 $first $second 'right tab keeps its document'
    if ((Send $left 2008) -ne 0 -or (Send $right 2008) -ne 2) { throw 'Tab focus must preserve both view carets' }
    if ([SelectionUi]::TabLabel($rightTabs, 0) -ne 'Untitled 2 *') { throw 'Unexpected right-document tab label' }
    SelectTab 0
    AssertState 0 0 $first $second 'mouse selects existing left document'
    Keys '^{TAB}'
    AssertState 0 0 $first $second 'Ctrl+Tab stays inside left group'
    Keys '^+{TAB}'
    AssertState 0 0 $first $second 'Ctrl+Shift+Tab stays inside left group'
    Keys '{F6}'
    AssertState 1 0 $first $second 'F6 synchronizes right focus and selected tab'
    Click $left
    AssertState 0 0 $first $second 'left pane click synchronizes selected tab'
    Click $right
    AssertState 1 0 $first $second 'right pane click synchronizes selected tab'
    Keys '^n'
    $third = Send $right 2357
    if ((Send $tabs 0x1304) -ne 1 -or (Send $rightTabs 0x1304) -ne 2) { throw 'New must stay in focused group' }
    Keys '^{TAB}'
    AssertState 1 0 $first $second 'Ctrl+Tab cycles right group independently'
    Keys '^+{TAB}'
    AssertState 1 1 $first $third 'Ctrl+Shift+Tab cycles right group independently'
    Post $window 0x111 1500
    if ([SelectionUi]::TabLabel($rightTabs, 0) -ne '[P] Untitled 3') { throw 'Pin must reorder within right group' }
    Post $window 0x111 1502
    AssertState 1 0 $first $third 'pinned tab cannot cross pin boundary'
    Post $window 0x111 1500
    Post $window 0x111 1502
    AssertState 1 1 $first $third 'unpin and reorder stay within right group'
    Post $rightTabs 0x100 0x25
    Post $rightTabs 0x101 0x25
    AssertState 1 0 $first $second 'native tab Left arrow stays within right group'
    Post $rightTabs 0x100 0x27
    Post $rightTabs 0x101 0x27
    AssertState 1 1 $first $third 'native tab Right arrow stays within right group'
    Post $window 0x111 1005
    AssertState 1 0 $first $second 'closing right tab selects its sibling'
    Post $window 0x111 1506
    AssertState 0 1 $second $second 'explicit Clone shares document in both groups'
    Post $window 0x111 1005
    AssertState 0 0 $first $second 'closing clone keeps original dirty document without prompt'
    $tabRect = [SelectionUi]::TabData($rightTabs, 0, $false)
    $contextX = [int](([BitConverter]::ToInt32($tabRect, 0) + [BitConverter]::ToInt32($tabRect, 8)) / 2)
    $contextY = [int](([BitConverter]::ToInt32($tabRect, 4) + [BitConverter]::ToInt32($tabRect, 12)) / 2)
    Post $rightTabs 0x205 0 (($contextY -shl 16) -bor $contextX)
    $popup = [SelectionUi]::Popup($window)
    $menu = [IntPtr](Send $popup 0x1E1)
    $compareState = [SelectionUi]::GetMenuState($menu, 1505, 0)
    if ($compareState -eq [uint32]::MaxValue -or ($compareState -band 3) -ne 0) {
        throw 'Cross-pane context comparison must be enabled for different documents'
    }
    Post $window 0x1F
    Write-Output 'PASS cross-pane context comparison uses previously focused document'
    Post $window 0x111 1070
    Start-Sleep -Milliseconds 600
    if ((Send $left 2357) -ne $first -or (Send $right 2357) -ne $second) { throw 'Compare changed group documents' }
    Post $window 0x111 1073
    if ($ChromeScreenshotDirectory) { CheckChrome }
    Post $window 0x10
    if (!$app.WaitForExit(10000)) { throw 'Isolated test app failed to close' }
    $saved = Get-Content (Join-Path $directory 'session.json') -Raw | ConvertFrom-Json
    if ($saved.pane_documents[1].Count -ne 1 -or $saved.documents.Count -lt 2) { throw 'Recovery lost group membership' }
    $app = [Diagnostics.Process]::Start($start)
    Start-Sleep -Seconds 2
    $app.Refresh()
    $window = $app.MainWindowHandle
    $left = [SelectionUi]::GetDlgItem($window, 101)
    $right = [SelectionUi]::GetDlgItem($window, 102)
    $tabs = [SelectionUi]::GetDlgItem($window, 302)
    $rightTabs = [SelectionUi]::GetDlgItem($window, 306)
    if ((Send $rightTabs 0x1304) -ne 1 -or (Send $right 2006) -ne 2) { throw 'Recovery failed to restore right group document' }
    [SelectionUi]::FocusEditor($window, $left)
    Keys '^{TAB}'
    if ((Send $left 2006) -ne 2) {
        throw "Recovery left mismatch length=$(Send $left 2006) selected=$(Send $tabs 0x130B) label=$([SelectionUi]::TabLabel($tabs,0)) saved=$($saved | ConvertTo-Json -Depth 5 -Compress)"
    }
    Post $window 0x10
    if (!$app.WaitForExit(10000)) { throw 'Recovered test app failed to close' }
    $damaged = Get-Content (Join-Path $directory 'session.json') -Raw | ConvertFrom-Json
    $damaged.pane_documents = @(@(0,0,999999), @(1,1))
    $damaged.pane_selected = @(999999,999999)
    $damaged.focused_pane = 999999
    $damaged | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $directory 'session.json')
    $app = [Diagnostics.Process]::Start($start)
    Start-Sleep -Seconds 2
    $app.Refresh()
    $window = $app.MainWindowHandle
    $tabs = [SelectionUi]::GetDlgItem($window, 302)
    $rightTabs = [SelectionUi]::GetDlgItem($window, 306)
    if ((Send $tabs 0x1304) + (Send $rightTabs 0x1304) -ne $damaged.documents.Count) {
        throw 'Damaged pane metadata lost or duplicated document tabs'
    }
    Post $window 0x10
    if (!$app.WaitForExit(10000)) { throw 'Metadata repair test app failed to close' }
    $repaired = Get-Content (Join-Path $directory 'session.json') -Raw | ConvertFrom-Json
    if (($repaired.documents.text -join '|') -ne ($damaged.documents.text -join '|')) { throw 'Pane metadata repair changed document text' }
    # Backward-compatible input and a real file open, only in this synthetic session.
    $legacy = Get-Content (Join-Path $directory 'session.json') -Raw | ConvertFrom-Json
    foreach ($field in @('pane_documents', 'pane_selected', 'focused_pane')) {
        $legacy.PSObject.Properties.Remove($field)
    }
    $legacy | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $directory 'session.json')
    $fixture = Join-Path $directory 'opened.txt'
    [IO.File]::WriteAllText([IO.Path]::GetFullPath($fixture), 'synthetic open')
    $start.ArgumentList.Add([IO.Path]::GetFullPath($fixture))
    $app = [Diagnostics.Process]::Start($start)
    Start-Sleep -Seconds 2
    $app.Refresh()
    $window = $app.MainWindowHandle
    $left = [SelectionUi]::GetDlgItem($window, 101)
    $right = [SelectionUi]::GetDlgItem($window, 102)
    $tabs = [SelectionUi]::GetDlgItem($window, 302)
    $rightTabs = [SelectionUi]::GetDlgItem($window, 306)
    if ((Send $tabs 0x1304) -ne $legacy.documents.Count + 1 -or (Send $rightTabs 0x1304) -ne 0) { throw 'Legacy recovery or file open lost tabs' }
    if ((Send $left 2006) -ne 14) { throw 'File open did not select its document' }
    Post $window 0x111 1504
    if ((Send $rightTabs 0x1304) -ne 1 -or (Send $right 2006) -ne 14) { throw 'Moving opened file failed' }
    Post $window 0x111 1005
    if ((Send $rightTabs 0x1304) -ne 0 -or (Send $tabs 0x1304) -ne $legacy.documents.Count) { throw 'Closing last group tab lost other documents' }
    Post $window 0x10
    if (!$app.WaitForExit(10000)) { throw 'Legacy test app failed to close' }
    $start.ArgumentList.RemoveAt(2)
    $start.ArgumentList[1] = Join-Path ([IO.Path]::GetFullPath($directory)) 'last-tab'
    $app = [Diagnostics.Process]::Start($start)
    Start-Sleep -Seconds 2
    $app.Refresh()
    $window = $app.MainWindowHandle
    $left = [SelectionUi]::GetDlgItem($window, 101)
    $right = [SelectionUi]::GetDlgItem($window, 102)
    $tabs = [SelectionUi]::GetDlgItem($window, 302)
    $rightTabs = [SelectionUi]::GetDlgItem($window, 306)
    $original = Send $left 2357
    Post $window 0x111 1504
    if ((Send $right 2357) -ne $original -or (Send $left 2357) -eq $original) { throw 'Moving last tab must not silently clone it' }
    Start-Sleep -Seconds 4
    Post $window 0x111 1050
    if ((Send $tabs 0x1304) -ne 2 -or (Send $rightTabs 0x1304) -ne 0) { throw 'Collapsing split lost a tab' }
    Start-Sleep -Seconds 4
    $autosaved = Get-Content (Join-Path $start.ArgumentList[1] 'session.json') -Raw | ConvertFrom-Json
    if ($autosaved.pane_documents[1].Count -ne 0) { throw 'Collapsing split must autosave without text edits' }
    Post $window 0x111 1070
    Start-Sleep -Seconds 4
    $autosaved = Get-Content (Join-Path $start.ArgumentList[1] 'session.json') -Raw | ConvertFrom-Json
    if ($autosaved.pane_documents[1].Count -ne 1) { throw 'Compare group movement must autosave without text edits' }
    Post $window 0x111 1050
    Post $window 0x111 1005
    Post $window 0x111 1005
    if ((Send $tabs 0x1304) -ne 1 -or (Send $left 2006) -ne 0) { throw 'Closing final tab must leave a usable blank document' }
    Post $window 0x10
    if (!$app.WaitForExit(10000)) { throw 'Last-tab test app failed to close' }
    Write-Output "All native pane-group, clone, compare, keyboard and recovery regressions passed. Chrome exercised=$([bool]$ChromeScreenshotDirectory)."
} finally {
    if (!$app.HasExited) { Stop-Process -Id $app.Id; $app.WaitForExit() }
    Remove-Item -LiteralPath $directory -Recurse -Force
}
