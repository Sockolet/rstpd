param([string]$Executable = "$PSScriptRoot\..\target\debug\rstpd.exe", [switch]$WorkflowsOnly, [switch]$LargeFilesOnly)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class RstpdSmoke {
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h, int id);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, string l);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetMenu(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetSubMenu(IntPtr menu, int index);
    [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] public static extern uint GetMenuItemID(IntPtr menu, int index);
    [DllImport("user32.dll")] public static extern uint GetMenuState(IntPtr menu, uint id, uint flags);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder b, int count);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect rect);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int width, int height, bool redraw);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern uint GetDoubleClickTime();
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetMenuString(IntPtr menu, uint item, StringBuilder text, int count, uint flags);
    public static void Click(IntPtr target, int packedPoint, int count) {
        for(int i=0;i<count;i++) {
            if(!PostMessage(target,(i%2==0)?0x201u:0x203u,(IntPtr)1,(IntPtr)packedPoint) ||
               !PostMessage(target,0x202,IntPtr.Zero,(IntPtr)packedPoint))
                throw new InvalidOperationException("Mouse click messages were not delivered.");
            System.Threading.Thread.Sleep(50);
        }
        System.Threading.Thread.Sleep(200);
    }
    public static void DoubleClick(IntPtr target, int packedPoint) {
        foreach(uint message in new uint[]{0x201,0x202,0x203,0x202})
            if(!PostMessage(target,message,(IntPtr)((message==0x202)?0:1),(IntPtr)packedPoint))
                throw new InvalidOperationException("Double-click messages were not delivered.");
        System.Threading.Thread.Sleep(200);
    }
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindow callback, IntPtr state);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumWindow callback, IntPtr state);
    [DllImport("user32.dll")] static extern int GetDlgCtrlID(IntPtr window);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder text, int count);
    delegate bool EnumWindow(IntPtr window, IntPtr state);
    public static IntPtr Dialog(int process) {return OwnedWindow(process,"#32770");}
    public static IntPtr Popup(int process) {return OwnedWindow(process,"#32768");}
    static IntPtr OwnedWindow(int process, string className) {
        IntPtr found=IntPtr.Zero;
        EnumWindows((window,state)=>{
            uint owner; GetWindowThreadProcessId(window,out owner);
            var name=new StringBuilder(128); GetClassName(window,name,128);
            if(owner==process && name.ToString()==className && IsWindowVisible(window)) {found=window;return false;}
            return true;
        },IntPtr.Zero);
        return found;
    }
    public static string DialogText(IntPtr dialog) {
        var output=new StringBuilder();
        EnumChildWindows(dialog,(window,state)=>{
            var text=new StringBuilder(4096);GetWindowText(window,text,text.Capacity);
            output.Append(GetDlgCtrlID(window)).Append(": ").Append(text).Append(" | ");return true;
        },IntPtr.Zero);
        return output.ToString();
    }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
}
'@
$root = Split-Path $PSScriptRoot -Parent
$directory = Join-Path $root "target\smoke-$([Guid]::NewGuid().ToString('N'))"
$firstProfile = Join-Path $directory 'fresh-profile'
$firstSessionDirectory = Join-Path $firstProfile 'rstpd'
$firstSession = Join-Path $firstSessionDirectory 'session.json'
$legacySessionDirectory = Join-Path $firstProfile 'RSTPad'
$legacySession = Join-Path $legacySessionDirectory 'session.json'
$workflowDirectory=Join-Path $directory 'workflows'
$workflowSession=Join-Path $workflowDirectory 'session'
$largeDirectory=Join-Path $directory 'large-search'
$largeSession=Join-Path $largeDirectory 'state'
New-Item -ItemType Directory $directory | Out-Null
Copy-Item "$root\tests\fixtures\before.json" (Join-Path $directory 'before.json')
Copy-Item "$root\tests\fixtures\after.json" (Join-Path $directory 'after.json')
foreach ($name in @('custom-language.xml','sample.rstlang','completion-api.xml','functions.rs','highlighting.md')) {
    Copy-Item (Join-Path $root "tests\fixtures\$name") (Join-Path $directory $name)
}
$exe = (Resolve-Path $Executable).Path
$script:process = $null
function Assert($condition, $message) { if (!$condition) { throw $message } }
function Assert-Running {
    $script:process.Refresh()
    if ($script:process.HasExited) {
        $diagnostic = $script:process.StandardError.ReadToEnd().Trim()
        throw "App exited unexpectedly (exit code $($script:process.ExitCode)).`n$diagnostic"
    }
}
function Start-Editor([bool]$fixtures, [bool]$defaultSession = $false, [string[]]$extraArguments = @()) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $exe
    $info.WorkingDirectory = Split-Path $exe -Parent
    $info.UseShellExecute = $false
    $info.RedirectStandardError = $true
    if ($defaultSession) {
        $info.Environment['LOCALAPPDATA'] = $firstProfile
    } else {
        $info.ArgumentList.Add('--session-dir')
        $info.ArgumentList.Add($directory)
    }
    if ($fixtures) {
        $info.ArgumentList.Add((Join-Path $directory 'before.json'))
        $info.ArgumentList.Add((Join-Path $directory 'after.json'))
    }
    foreach ($argument in $extraArguments) { $info.ArgumentList.Add($argument) }
    $script:process = [Diagnostics.Process]::Start($info)
    for ($i = 0; $i -lt 100; $i++) {
        Assert-Running
        if ($script:process.MainWindowHandle -ne 0) { break }
        Start-Sleep -Milliseconds 100
    }
    Assert-Running
    $script:window = $script:process.MainWindowHandle
    Assert ($script:window -ne 0) 'Main window did not appear.'
    Start-Sleep -Milliseconds 300
    Assert-Running
    Assert ((Caption $script:window) -match ' - rstpd$') 'Application window still uses the old tool name.'
}
function Close-Editor {
    [RstpdSmoke]::PostMessage($script:window,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
    Assert ($script:process.WaitForExit(10000)) 'App did not flush and close.'
    Assert ($script:process.ExitCode -eq 0) "App exited with code $($script:process.ExitCode)."
}
function Command([int]$id) {
    $button=Control $id
    if ($button -ne [IntPtr]::Zero -and $id -in @(1001,1002,1003,1040,1050,1051,1070,1082)) {
        [RstpdSmoke]::PostMessage($button,0xF5,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
    } else {
        [RstpdSmoke]::PostMessage($script:window,0x111,[IntPtr]$id,[IntPtr]::Zero) | Out-Null
    }
    Start-Sleep -Milliseconds 200
}
function Control([int]$id) { [RstpdSmoke]::GetDlgItem($script:window,$id) }
function Number([IntPtr]$handle,[uint32]$message,[long]$w = 0,[long]$l = 0) {
    [RstpdSmoke]::SendMessage($handle,$message,[IntPtr]$w,[IntPtr]$l).ToInt64()
}
function Wait-Until([scriptblock]$condition,[string]$message) {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    do {
        Assert-Running
        if (& $condition) { return }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    throw $message
}
function Check-CharacterCount([int]$total,[int]$selected=0) {
    $noun=if($total -eq 1){'character'}else{'characters'}
    $text=if($selected -gt 0){"$selected of $total $noun"}else{"$total $noun"}
    $pattern='^Ln \d+, Col \d+\s+\|\s+'+[Regex]::Escape($text)+'\s+\|'
    Wait-Until { (Caption (Control 303)) -match $pattern } "Status bar did not display '$text' next to line/column."
}
function Type-Text([string]$text) {
    foreach ($character in $text.ToCharArray()) {
        [RstpdSmoke]::SendMessage((Control 101),0x102,[IntPtr][int]$character,[IntPtr]::Zero) | Out-Null
    }
}
function Caption([IntPtr]$handle) {
    $text = [Text.StringBuilder]::new(4096)
    [RstpdSmoke]::GetWindowText($handle,$text,$text.Capacity) | Out-Null
    $text.ToString()
}
function Dismiss-WorkflowDialog([string]$phase) {
    try {Wait-Until {
        $dialog=[RstpdSmoke]::Dialog($script:process.Id)
        $dialog -ne [IntPtr]::Zero -and (
            [RstpdSmoke]::GetDlgItem($dialog,7) -ne [IntPtr]::Zero -or
            [RstpdSmoke]::GetDlgItem($dialog,1) -ne [IntPtr]::Zero -or
            [RstpdSmoke]::GetDlgItem($dialog,2) -ne [IntPtr]::Zero
        )
    } 'Expected a fully initialized conflict/error dialog.'}
    catch {throw "$phase dialog unavailable: $([RstpdSmoke]::DialogText([RstpdSmoke]::Dialog($script:process.Id)))"}
    $dialog=[RstpdSmoke]::Dialog($script:process.Id)
    $no=[RstpdSmoke]::GetDlgItem($dialog,7)
    $button=if($no -ne [IntPtr]::Zero){$no}else{[RstpdSmoke]::GetDlgItem($dialog,1)}
    if($button -eq [IntPtr]::Zero){$button=[RstpdSmoke]::GetDlgItem($dialog,2)}
    Assert ($button -ne [IntPtr]::Zero) "$phase dialog has no expected response button."
    [RstpdSmoke]::PostMessage($button,0xF5,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
    try {Wait-Until { [RstpdSmoke]::Dialog($script:process.Id) -eq [IntPtr]::Zero } 'Dialog did not close.'}
    catch {throw "$phase dialog did not close: $([RstpdSmoke]::DialogText([RstpdSmoke]::Dialog($script:process.Id)))"}
}
function Tab-Point([int]$index) {
    $scale=[RstpdSmoke]::GetDpiForWindow($script:window)/96
    ([int](16*$scale) -shl 16) -bor [int]((95+$index*190)*$scale)
}
function Select-Tab([int]$index) {
    Number (Control 302) 0x201 1 (Tab-Point $index) | Out-Null
    Number (Control 302) 0x202 0 (Tab-Point $index) | Out-Null
    Start-Sleep -Milliseconds 300
}
function Tab-Menu([int]$index,[int]$action) {
    $title=Caption $script:window
    [RstpdSmoke]::PostMessage((Control 302),0x204,[IntPtr]2,[IntPtr](Tab-Point $index)) | Out-Null
    [RstpdSmoke]::PostMessage((Control 302),0x205,[IntPtr]::Zero,[IntPtr](Tab-Point $index)) | Out-Null
    Wait-Until { [RstpdSmoke]::Popup($script:process.Id) -ne [IntPtr]::Zero } 'Tab context menu did not open.'
    $popup=[RstpdSmoke]::Popup($script:process.Id)
    $menu=[IntPtr](Number $popup 0x1E1)
    Assert ($menu -ne [IntPtr]::Zero) 'Tab popup did not expose its native menu.'
    foreach ($entry in @(@(1504,'Open in split view'),@(1505,'Compare with current view'))) {
        $label=[Text.StringBuilder]::new(128)
        [RstpdSmoke]::GetMenuString($menu,$entry[0],$label,$label.Capacity,0) | Out-Null
        Assert ($label.ToString() -eq $entry[1]) "Missing context action: $($entry[1])"
    }
    Assert ((Caption $script:window) -eq $title) 'Right-click changed the active document.'
    if($action -eq 0){
        Assert (([RstpdSmoke]::GetMenuState($menu,1505,0) -band 3) -ne 0) 'Self-comparison must be disabled.'
        [RstpdSmoke]::PostMessage($popup,0x100,[IntPtr]0x1B,[IntPtr]::Zero) | Out-Null
    }else{
        for($step=0;$step -lt [RstpdSmoke]::GetMenuItemCount($menu)+1;$step++){
            if(([RstpdSmoke]::GetMenuState($menu,$action,0) -band 128) -ne 0){break}
            [RstpdSmoke]::PostMessage($popup,0x100,[IntPtr]0x28,[IntPtr]::Zero) | Out-Null
            Start-Sleep -Milliseconds 60
        }
        Assert (([RstpdSmoke]::GetMenuState($menu,$action,0) -band 128) -ne 0) 'Could not select the requested context action.'
        [RstpdSmoke]::PostMessage($popup,0x100,[IntPtr]0x0D,[IntPtr]::Zero) | Out-Null
    }
    Wait-Until { [RstpdSmoke]::Popup($script:process.Id) -eq [IntPtr]::Zero } 'Tab context menu did not close.'
    Start-Sleep -Milliseconds 250
}
function Editor-Text([int]$id) {
    $editor=Control $id
    [Text.Encoding]::UTF8.GetString([byte[]]@(for($position=0;$position -lt (Number $editor 2006);$position++){Number $editor 2007 $position}))
}
function Check-Workflows {
    New-Item -ItemType Directory $workflowDirectory | Out-Null
    $alpha=Join-Path $workflowDirectory 'alpha.txt'
    $beta=Join-Path $workflowDirectory 'beta.txt'
    $disk=Join-Path $workflowDirectory 'disk-only.txt'
    [IO.File]::WriteAllText($alpha,"anchor`nsecond`n")
    [IO.File]::WriteAllText($beta,"second`nanchor`n")
    [IO.File]::WriteAllText($disk,"needle cafe needle`n")
    Start-Editor $false $false @('--session-dir',$workflowSession,$alpha,$beta)
    Assert ((Number (Control 302) 0x1304) -eq 2) 'Workflow fixture did not open two tabs.'
    Command 1500
    Assert ((Number (Control 302) 0x130B) -eq 0) 'Pinning did not move the active tab left.'
    Assert ((Caption $script:window) -match '^beta.txt') 'Pinning changed the active document.'
    Command 1502
    Assert ((Number (Control 302) 0x130B) -eq 0) 'Pinned tab crossed into the unpinned group.'
    Command 1500
    Command 1502
    Assert ((Number (Control 302) 0x130B) -eq 1) 'Move-right command did not reorder the tab.'
    Number (Control 302) 0x201 1 (Tab-Point 1) | Out-Null
    Number (Control 302) 0x200 1 (Tab-Point 0) | Out-Null
    Number (Control 302) 0x202 0 (Tab-Point 0) | Out-Null
    Wait-Until { (Number (Control 302) 0x130B) -eq 0 } 'Dragging did not reorder tabs.'
    Command 1500
    Command 1001
    Select-Tab 0
    Number (Control 101) 2160 1 4 | Out-Null
    Tab-Menu 1 1504
    Assert ([RstpdSmoke]::IsWindowVisible((Control 102))) 'Open in split view did not open the other pane.'
    Assert ((Editor-Text 101) -eq "second`nanchor`n" -and (Editor-Text 102) -eq "anchor`nsecond`n") 'Split context action did not preserve the current document and open the clicked one.'
    Assert ((Number (Control 101) 2143) -eq 1 -and (Number (Control 101) 2145) -eq 4) 'Split context action lost the active selection.'
    # Each pane owns its tabs: the clicked tab moves to the right pane's tab bar and takes focus.
    Assert ((Number (Control 302) 0x1304) -eq 2 -and (Number (Control 306) 0x1304) -eq 1) 'Open in split view did not move the tab to the right pane.'
    Wait-Until { (Caption $script:window) -match '^alpha.txt' } 'Open in split view did not focus the moved tab.'
    Tab-Menu 0 1505
    Wait-Until { ((Number (Control 101) 2046 0) -bor (Number (Control 101) 2046 1)) -ne 0 } 'Context comparison did not complete.'
    Assert ((Editor-Text 101) -eq "anchor`nsecond`n" -and (Editor-Text 102) -eq "second`nanchor`n") 'Compare with current view did not compare the previously active right pane with the clicked tab.'
    Assert ((Number (Control 302) 0x1304) + (Number (Control 306) 0x1304) -eq 3) 'Context actions duplicated document tabs.'
    Tab-Menu 1 0
    Tab-Menu 0 1504
    Assert (((Number (Control 101) 2046 0) -band ((1-shl 20)-bor(1-shl 21)-bor(1-shl 22))) -eq 0) 'Opening a normal split left stale comparison marks.'
    Command 1050
    Select-Tab 2
    Command 1005
    Select-Tab 0
    $scale=[RstpdSmoke]::GetDpiForWindow($script:window)/96
    $blank=([int](16*$scale) -shl 16) -bor [int](390*$scale)
    $tabRect=[RstpdSmoke+Rect]::new()
    [RstpdSmoke]::GetWindowRect((Control 302),[ref]$tabRect) | Out-Null
    $screenBlank=(($tabRect.top+[int](16*$scale)) -shl 16) -bor (($tabRect.left+[int](390*$scale)) -band 0xffff)
    $hit=Number (Control 302) 0x84 0 $screenBlank
    Assert ($hit -eq 1) "Empty tab space is not mouse-hit-testable (WM_NCHITTEST returned $hit instead of HTCLIENT)."
    [RstpdSmoke]::Click((Control 302),(Tab-Point 0),2)
    Assert ((Number (Control 302) 0x1304) -eq 2) 'Double-clicking a tab label created a document.'
    [RstpdSmoke]::Click((Control 302),$blank,1)
    Assert ((Number (Control 302) 0x1304) -eq 2) 'Single-clicking empty tab space created a document.'
    Start-Sleep -Milliseconds ([RstpdSmoke]::GetDoubleClickTime()+50)
    [RstpdSmoke]::Click((Control 302),$blank,2)
    Wait-Until { (Number (Control 302) 0x1304) -eq 3 } 'Double-click after the last tab did not create an untitled document.'
    Assert ((Number (Control 101) 2006) -eq 0) 'Double-click-created tab was not empty.'
    Command 1005
    [RstpdSmoke]::DoubleClick((Control 302),$blank)
    Wait-Until { (Number (Control 302) 0x1304) -eq 3 } 'A Windows-recognized double-click was lost in the message loop.'
    Command 1005
    Select-Tab 1
    Number (Control 101) 2160 2 6 | Out-Null
    [IO.File]::WriteAllText($alpha,"external text`n")
    Wait-Until { (Number (Control 101) 2006) -eq 14 -and (Caption (Control 303)) -match 'Reloaded external' } 'Clean tab did not automatically reload.'
    Assert ((Number (Control 101) 2143) -eq 2 -and (Number (Control 101) 2145) -eq 6) 'External reload lost the selection.'
    Assert ((Number (Control 101) 2159) -eq 0) 'Automatic reload marked the buffer modified.'
    Command 1503
    [IO.File]::WriteAllText($alpha,"monitor paused`n")
    Start-Sleep -Milliseconds 1200
    Assert ((Number (Control 101) 2006) -eq 14) 'Disabled monitoring reloaded a file.'
    Command 1503
    Wait-Until { (Number (Control 101) 2006) -eq 15 } 'Re-enabling monitoring did not refresh a changed file.'
    [IO.File]::WriteAllText($alpha,"external text`n")
    Wait-Until { (Number (Control 101) 2006) -eq 14 } 'Monitoring did not resume normally.'
    Number (Control 101) 2078 | Out-Null
    Type-Text 'local'
    Number (Control 101) 2079 | Out-Null
    $editedLength=Number (Control 101) 2006
    [IO.File]::WriteAllText($alpha,"next external text`n")
    Dismiss-WorkflowDialog 'External reload'
    Assert ((Number (Control 101) 2006) -eq $editedLength) 'Declining reload discarded unsaved edits.'
    Command 1003
    Dismiss-WorkflowDialog 'Save conflict'
    Assert ([IO.File]::ReadAllText($alpha) -eq "next external text`n") 'Declined save overwrote an external change.'
    Command 1010
    Assert ((Number (Control 101) 2159) -eq 0) 'Undo did not return the monitored document to its save point.'
    [IO.File]::WriteAllText($alpha,"fresh external text`n")
    Wait-Until { (Number (Control 101) 2006) -eq 20 } 'Monitoring did not resume after an external conflict.'
    Command 1180
    [RstpdSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'needle') | Out-Null
    [RstpdSmoke]::SendMessage((Control 410),0x0c,[IntPtr]::Zero,$workflowDirectory) | Out-Null
    [RstpdSmoke]::SendMessage((Control 411),0x0c,[IntPtr]::Zero,'disk-only.txt') | Out-Null
    Command 1181
    Wait-Until { (Caption (Control 305)) -match '^2 matches in 1 document' } 'Find in Files did not return matches from a closed file.'
    Command 1173
    Assert ((Caption $script:window) -match '^disk-only.txt') 'A disk result did not open its file.'
    Assert ((Number (Control 101) 2143) -eq 0 -and (Number (Control 101) 2145) -eq 6) 'Disk-result navigation was not exact.'
    Number (Control 101) 2078 | Out-Null
    Type-Text 'unsaved edits'
    Number (Control 101) 2079 | Out-Null
    $editedLength=Number (Control 101) 2006
    Command 1173
    Dismiss-WorkflowDialog 'Disk result'
    Assert ((Number (Control 101) 2006) -eq $editedLength) 'Disk result replaced a dirty buffer.'
    Command 1010
    Command 1160
    Command 1176
    Command 1001
    Type-Text "first`nsecond`nthird`n"
    Command 1001
    Type-Text "third`nfirst`nsecond`n"
    Command 1501
    Command 1070
    Wait-Until { ((Number (Control 101) 2046 0) -band (1 -shl 22)) -ne 0 -and ((Number (Control 102) 2046 2) -band (1 -shl 22)) -ne 0 } 'Comparison did not detect moved lines.'
    Assert ((Number (Control 101) 2006) -eq 19 -and (Number (Control 102) 2006) -eq 19) 'Comparison inserted spacer text into documents.'
    $leftRect=[RstpdSmoke+Rect]::new();$rightRect=[RstpdSmoke+Rect]::new()
    [RstpdSmoke]::GetWindowRect((Control 101),[ref]$leftRect) | Out-Null
    [RstpdSmoke]::GetWindowRect((Control 102),[ref]$rightRect) | Out-Null
    Assert ($rightRect.top -gt $leftRect.top) 'Leading inserted lines are not visually aligned.'
    Number (Control 101) 2160 0 5 | Out-Null
    Number (Control 102) 2160 13 18 | Out-Null
    Command 1510
    Wait-Until { (Caption (Control 303)) -match '^.*0 difference groups' } 'Selected-line comparison included unselected text.'
    Command 1073
    # Collapse the comparison split so tab 1 is clicked on the full-width left tab bar.
    Command 1050
    Select-Tab 1
    Type-Text 'unsaved'
    Command 1512
    Wait-Until { [RstpdSmoke]::IsWindowVisible((Control 102)) -and (Caption (Control 303)) -match 'difference groups|Difference \d+ of \d+' } 'Last-saved comparison did not complete.'
    Assert ((Number (Control 102) 2006) -eq 20) 'Last-saved comparison did not use disk contents.'
    Assert ([IO.File]::ReadAllText($alpha) -eq "fresh external text`n") 'Comparison changed the original file.'
    Close-Editor
    $saved=Get-Content (Join-Path $workflowSession 'session.json') -Raw | ConvertFrom-Json
    Assert ($saved.documents[$saved.pane_documents[0][0]].title -eq 'beta.txt' -and $saved.documents[$saved.pane_documents[0][0]].pinned) 'Pinned order was not persisted.'
    Assert ($saved.monitor_files -and $saved.compare_options.align -and $saved.compare_options.detect_moves) 'Workflow preferences were not persisted.'
    Start-Editor $false $false @('--session-dir',$workflowSession)
    Assert ([RstpdSmoke]::IsWindowVisible((Control 102))) 'Recovery did not restore the split pane groups.'
    # Collapse the restored split so tab 0 is clicked on the full-width left tab bar.
    Command 1050
    Select-Tab 0
    Command 1502
    Assert ((Number (Control 302) 0x130B) -eq 0) 'Restored pinned tab no longer stayed left.'
    Close-Editor
}
function Check-LanguageMenu([bool]$custom) {
    $menu=[RstpdSmoke]::GetSubMenu([RstpdSmoke]::GetMenu($script:window),5)
    Assert ($menu -ne [IntPtr]::Zero) 'Language menu is missing.'
    Assert ([RstpdSmoke]::GetMenuItemID($menu,0) -eq 2000) 'Plain text is not directly accessible.'
    $count=[RstpdSmoke]::GetMenuItemCount($menu)
    Assert ($count -eq $(if ($custom) {15} else {14})) 'Language menu contains unexpected top-level categories.'
    $letters=@('a','d','g','j','m','p','s','v')
    $ids=[Collections.Generic.List[uint32]]::new()
    $ids.Add(2000)
    for ($index=0; $index -lt $letters.Count; $index++) {
        $mnemonic=[RstpdSmoke]::SendMessage($script:window,0x120,[IntPtr][int][char]$letters[$index],$menu).ToInt64()
        Assert (($mnemonic -shr 16) -eq 2 -and ($mnemonic -band 0xffff) -eq ($index+2)) 'Alphabetical language group has an incorrect label or mnemonic.'
        $group=[RstpdSmoke]::GetSubMenu($menu,$index+2)
        Assert ($group -ne [IntPtr]::Zero) 'Language letter group is missing.'
        for ($item=0; $item -lt [RstpdSmoke]::GetMenuItemCount($group); $item++) {
            $ids.Add([RstpdSmoke]::GetMenuItemID($group,$item))
        }
    }
    if ($custom) {
        $group=[RstpdSmoke]::GetSubMenu($menu,10)
        Assert ($group -ne [IntPtr]::Zero) 'User-defined languages are not separate.'
        for ($item=0; $item -lt [RstpdSmoke]::GetMenuItemCount($group); $item++) {
            $ids.Add([RstpdSmoke]::GetMenuItemID($group,$item))
        }
    }
    $sorted=@($ids | Sort-Object)
    Assert ($sorted.Count -ge 100) 'Languages were lost during menu regrouping.'
    for ($index=0; $index -lt $sorted.Count; $index++) {
        Assert ($sorted[$index] -eq 2000+$index) 'A language command is missing, duplicated or misnumbered.'
    }
    Assert ([RstpdSmoke]::GetMenuItemID($menu,$count-3) -eq 1400) 'Language management commands are not at the bottom.'
}
function Check-LargeFileSearch {
    New-Item -ItemType Directory $largeDirectory | Out-Null
    $path=Join-Path $largeDirectory 'large.log'
    $size=50*1024*1024
    $line=('ordinary log entry'.PadRight(127,'x'))+"`n"
    $chunk=[Text.Encoding]::UTF8.GetBytes($line*8192)
    $marker=[Text.Encoding]::UTF8.GetBytes("SEARCH-98765`n")
    $last=$size-$marker.Length
    $stream=[IO.File]::Create($path)
    try {
        for($block=0;$block -lt 50;$block++){$stream.Write($chunk,0,$chunk.Length)}
        $stream.Position=0;$stream.Write($marker,0,$marker.Length)
        $stream.Position=$last;$stream.Write($marker,0,$marker.Length)
    }finally{$stream.Dispose()}
    Start-Editor $false $false @('--session-dir',$largeSession,$path)
    Wait-Until { (Number (Control 101) 2006) -eq $size } 'The 50MiB log did not open.'
    Command 1040
    [RstpdSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'SEARCH-98765') | Out-Null
    Command 1041
    Wait-Until { (Number (Control 101) 2143) -eq 0 } 'Find Next failed on a 50MiB log.'
    Command 1041
    Wait-Until { (Number (Control 101) 2143) -eq $last } 'Find Next did not reach the end of a 50MiB log.'
    Command 1042
    Wait-Until { (Number (Control 101) 2143) -eq 0 } 'Find Previous failed on a 50MiB log.'
    Command 1170
    Wait-Until { (Caption (Control 305)) -match '^2 matches in 1 document' } 'Find All failed on a 50MiB log.'
    Command 1173
    Command 1173
    Wait-Until { (Number (Control 101) 2143) -eq $last } 'Large-file result navigation was not exact.'
    [RstpdSmoke]::SendMessage((Control 402),0x0c,[IntPtr]::Zero,'FOUND!-98765') | Out-Null
    Command 1044
    Wait-Until { (Number (Control 101) 2007 0) -eq 70 } 'Replace All kept the old 16MiB limit.'
    Command 1010
    Wait-Until { (Number (Control 101) 2007 0) -eq 83 } 'Large replacement did not undo.'
    Command 1180
    [RstpdSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'SEARCH-98765') | Out-Null
    [RstpdSmoke]::SendMessage((Control 410),0x0c,[IntPtr]::Zero,$largeDirectory) | Out-Null
    [RstpdSmoke]::SendMessage((Control 411),0x0c,[IntPtr]::Zero,'large.log') | Out-Null
    Number (Control 412) 0xF1 0 | Out-Null
    Command 1175
    Wait-Until { (Caption (Control 305)) -match '^Search results cleared' } 'Could not clear earlier large-file results.'
    Command 1181
    Wait-Until { (Caption (Control 305)) -match '^2 matches in 1 document' } 'Directory search failed on a 50MiB log.'
    Command 1173
    Command 1173
    Wait-Until { (Number (Control 101) 2143) -eq $last } 'Large directory-result navigation was not exact.'
    Close-Editor
}
function Check-SearchControls {
    $ids=@(401,402,403,404,405,406,407,408,1041,1042,1043,1044,1160,1170,1171)
    $rects=@()
    foreach ($id in $ids) {
        $control=Control $id
        Assert ([RstpdSmoke]::IsWindowVisible($control)) "Search control $id is not visible."
        $rect=[RstpdSmoke+Rect]::new()
        [RstpdSmoke]::GetWindowRect($control,[ref]$rect) | Out-Null
        $rects+=@{ Id=$id; Rect=$rect }
    }
    $frame=[RstpdSmoke+Rect]::new()
    [RstpdSmoke]::GetWindowRect($script:window,[ref]$frame) | Out-Null
    for ($index=0; $index -lt $rects.Count; $index++) {
        $a=$rects[$index].Rect
        Assert ($a.left -ge $frame.left -and $a.right -le $frame.right) "Search control $($rects[$index].Id) is clipped."
        for ($other=$index+1; $other -lt $rects.Count; $other++) {
            $b=$rects[$other].Rect
            $overlaps=$a.left -lt $b.right -and $b.left -lt $a.right -and $a.top -lt $b.bottom -and $b.top -lt $a.bottom
            Assert (!$overlaps) "Search controls $($rects[$index].Id) and $($rects[$other].Id) overlap."
        }
    }
    Assert ((Caption (Control 406)) -eq 'Find:') 'The Find field has no permanent label.'
    Assert ((Caption (Control 407)) -eq 'Replace:') 'The Replace field has no permanent label.'
    $bitmap=[Drawing.Bitmap]::new($frame.right-$frame.left,$frame.bottom-$frame.top)
    try {
        $graphics=[Drawing.Graphics]::FromImage($bitmap)
        $dc=$graphics.GetHdc()
        try { Assert ([RstpdSmoke]::PrintWindow($script:window,$dc,2)) 'Cannot capture search controls.' }
        finally { $graphics.ReleaseHdc($dc);$graphics.Dispose() }
        foreach ($id in @(401,402,1170,1171,1160,1176)) {
            $rect=[RstpdSmoke+Rect]::new()
            [RstpdSmoke]::GetWindowRect((Control $id),[ref]$rect) | Out-Null
            $x=$rect.left-$frame.left
            $y=$rect.top-$frame.top
            $edge=$bitmap.GetPixel($x,$y+[int](($rect.bottom-$rect.top)/2)).ToArgb()
            $surface=$bitmap.GetPixel($x+5,$y+5).ToArgb()
            Assert ($edge -ne $surface) "Control $id has no visible edge."
        }
    } finally { $bitmap.Dispose() }
}
function Screenshot([string]$name) {
    $previous = [RstpdSmoke]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $renderers = @()
    try {
        # DirectWrite capture can be blank on an inactive/remote desktop. Use GDI only for the capture.
        foreach ($id in @(101,102,104,105)) {
            $control = Control $id
            $renderers += @{ Handle=$control; Technology=(Number $control 2631) }
            Number $control 2630 0 | Out-Null
        }
        $rect = [RstpdSmoke+Rect]::new()
        [RstpdSmoke]::GetWindowRect($script:window,[ref]$rect) | Out-Null
        $bitmap = [Drawing.Bitmap]::new($rect.right-$rect.left,$rect.bottom-$rect.top)
        try {
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            $dc = $graphics.GetHdc()
            try { Assert ([RstpdSmoke]::PrintWindow($script:window,$dc,2)) 'Window capture failed.' }
            finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
            $bitmap.Save((Join-Path $root "target\$name"),[Drawing.Imaging.ImageFormat]::Png)
        } finally { $bitmap.Dispose() }
    } finally {
        foreach ($renderer in $renderers) { Number $renderer.Handle 2630 $renderer.Technology | Out-Null }
        if ($previous -ne [IntPtr]::Zero) { [RstpdSmoke]::SetThreadDpiAwarenessContext($previous) | Out-Null }
    }
}
# Scintilla reports physical pixels; mouse messages must use the same DPI context.
$originalDpiContext=[RstpdSmoke]::SetThreadDpiAwarenessContext([IntPtr](-4))
try {
    if ($WorkflowsOnly) {Check-Workflows; Write-Host 'PASS: New editor workflows.';return}
    if ($LargeFilesOnly) {Check-LargeFileSearch; Write-Host 'PASS: Native50MiB find/replace, Find All and directory search.';return}
    Start-Editor $false $true
    Assert ((Number (Control 302) 0x1304) -eq 1) 'First launch must create one untitled tab.'
    Assert ((Number (Control 101) 2006) -eq 0) 'First-launch document must be empty.'
    Check-CharacterCount 0
    foreach ($character in 'First-launch text'.ToCharArray()) {
        [RstpdSmoke]::SendMessage((Control 101),0x102,[IntPtr][int]$character,[IntPtr]::Zero) | Out-Null
    }
    Check-CharacterCount ('First-launch text'.Length)
    Number (Control 101) 2160 5 0 | Out-Null
    Check-CharacterCount ('First-launch text'.Length) 5
    Number (Control 101) 2013 | Out-Null
    Check-CharacterCount ('First-launch text'.Length) ('First-launch text'.Length)
    Number (Control 101) 2160 0 0 | Out-Null
    Check-CharacterCount ('First-launch text'.Length)
    Number (Control 101) 2078 | Out-Null
    Type-Text '!'
    Number (Control 101) 2079 | Out-Null
    Check-CharacterCount ('First-launch text'.Length+1)
    Command 1010
    Check-CharacterCount ('First-launch text'.Length)
    Command 1304
    Command 1305
    Command 1306
    Assert ((Number (Control 101) 2020) -eq 1 -and (Number (Control 101) 2355) -eq 1) 'Show all characters did not enable whitespace and EOL marks.'
    Assert ((Number (Control 101) 2133) -eq 3 -and (Number (Control 101) 2461) -eq 1) 'Indent/wrap markers were not enabled.'
    $viewMenu=[RstpdSmoke]::GetSubMenu([RstpdSmoke]::GetMenu($script:window),3)
    $symbolsMenu=[RstpdSmoke]::GetSubMenu($viewMenu,([RstpdSmoke]::GetMenuItemCount($viewMenu)-1))
    foreach ($id in @(1300,1301,1302,1303,1304,1305,1306)) {
        Assert (([RstpdSmoke]::GetMenuState($symbolsMenu,$id,0) -band 8) -ne 0) 'Symbol menu checkmarks did not match the settings.'
    }
    Command 1301
    Assert (([RstpdSmoke]::GetMenuState($symbolsMenu,1304,0) -band 8) -eq 0) 'Show all remained checked after disabling one character option.'
    Command 1304
    Close-Editor
    $firstRecovery = Get-Content -LiteralPath $firstSession -Raw | ConvertFrom-Json
    Assert ($firstRecovery.documents.Count -eq 1) 'First-launch recovery lost the untitled tab.'
    Assert ($null -eq $firstRecovery.documents[0].path) 'First-launch tab must not require a filename.'
    Assert ($firstRecovery.documents[0].text -eq 'First-launch text') 'First-launch editing or recovery failed.'
    Assert ($firstRecovery.show_symbols.whitespace -and $firstRecovery.show_symbols.eol -and $firstRecovery.show_symbols.non_printing -and $firstRecovery.show_symbols.controls) 'Symbol preferences were not persisted.'

    Start-Editor $false $true
    Assert ((Number (Control 302) 0x1304) -eq 1) 'No-argument relaunch must restore the untitled tab.'
    Assert ((Number (Control 101) 2006) -eq 'First-launch text'.Length) 'No-argument relaunch lost recovered text.'
    Check-CharacterCount ('First-launch text'.Length)
    Assert ((Number (Control 101) 2020) -eq 1 -and (Number (Control 101) 2355) -eq 1) 'Symbol preferences were not restored.'
    Close-Editor

    New-Item -ItemType Directory -Force $legacySessionDirectory | Out-Null
    foreach ($name in @('session.json','session.lock')) {
        Move-Item -LiteralPath (Join-Path $firstSessionDirectory $name) -Destination (Join-Path $legacySessionDirectory $name)
    }
    Remove-Item -LiteralPath $firstSessionDirectory
    Start-Editor $false $true
    Assert ((Number (Control 101) 2006) -eq 'First-launch text'.Length) 'Renamed app did not restore legacy recovery data.'
    Close-Editor
    Assert (!(Test-Path -LiteralPath $firstSession)) 'Legacy recovery was unexpectedly moved or replaced.'
    $legacyRecovery = Get-Content -LiteralPath $legacySession -Raw | ConvertFrom-Json
    Assert ($legacyRecovery.documents[0].text -eq 'First-launch text') 'Legacy recovery contents changed during the rename.'

    New-Item -ItemType Directory -Force $firstSessionDirectory | Out-Null
    @{ version = 1; documents = @(); active = 0; theme = 'system' } |
        ConvertTo-Json | Set-Content -LiteralPath $firstSession -Encoding utf8
    Start-Editor $false $true
    Assert ((Number (Control 302) 0x1304) -eq 1) 'An empty saved session must create an untitled tab.'
    Assert ((Number (Control 101) 2006) -eq 0) 'Empty-session startup must create a blank document.'
    Close-Editor

    $unicodeText='A'+[char]0xE9+[char]::ConvertFromUtf32(0x1F680)+"`r`n"+'e'+[char]0x301+"`t"+[char]0+[char]0x4E2D
    $unicodePath=Join-Path $directory 'character-counts.txt'
    [IO.File]::WriteAllText($unicodePath,$unicodeText,[Text.UTF8Encoding]::new($false))
    Start-Editor $false $true @($unicodePath)
    Check-CharacterCount 10
    Number (Control 101) 2160 3 7 | Out-Null
    Check-CharacterCount 10 1
    Command 1001
    Check-CharacterCount 0
    Command 1005
    Check-CharacterCount 10
    Close-Editor

    Start-Editor $true
    Assert ((Number (Control 302) 0x1304) -eq 2) 'Expected two file tabs.'
    Check-LanguageMenu $false
    Command 1304
    Command 1305
    Command 1306
    Command 1050
    Assert ((Number (Control 102) 2020) -eq 1 -and (Number (Control 102) 2355) -eq 1) 'Show symbols was not applied to the split pane.'
    # Split moves the active tab (after.json) to the focused right pane; before.json stays left.
    $leftCount=[IO.File]::ReadAllText((Join-Path $directory 'before.json')).Length
    $rightCount=[IO.File]::ReadAllText((Join-Path $directory 'after.json')).Length
    Number (Control 101) 2160 0 3 | Out-Null
    Number (Control 102) 2160 0 4 | Out-Null
    Check-CharacterCount $rightCount 4
    [RstpdSmoke]::PostMessage((Control 102),0x100,[IntPtr]117,[IntPtr]::Zero) | Out-Null
    Check-CharacterCount $leftCount 3
    [RstpdSmoke]::PostMessage((Control 101),0x100,[IntPtr]117,[IntPtr]::Zero) | Out-Null
    Check-CharacterCount $rightCount 4
    Number (Control 101) 2160 0 0 | Out-Null
    Command 1050
    Command 1040
    [RstpdSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'application') | Out-Null
    Command 1170
    Wait-Until { (Caption (Control 305)) -match '^1 match in 1 document' } 'Find All in current document did not show its match.'
    Assert ([RstpdSmoke]::IsWindowVisible((Control 105))) 'The search-results panel is not visible.'
    Assert ((Number (Control 105) 2140) -eq 1) 'Search results are not read-only.'
    Command 1171
    Wait-Until { (Caption (Control 305)) -match '^2 matches in 2 documents' } 'Find All did not include both open documents.'
    Command 1173
    Assert ((Caption $script:window) -match '^before.json') 'First search result did not switch to its document.'
    $expectedMatch=[IO.File]::ReadAllText((Join-Path $directory 'before.json')).IndexOf('application')
    Assert ((Number (Control 101) 2143) -eq $expectedMatch -and (Number (Control 101) 2145) -eq $expectedMatch+11) 'Search result did not select the exact matching range.'
    Check-CharacterCount ([IO.File]::ReadAllText((Join-Path $directory 'before.json')).Length) 11
    Command 1173
    Assert ((Caption $script:window) -match '^after.json') 'Next result did not switch to the next document.'
    Command 1174
    Assert ((Caption $script:window) -match '^before.json') 'Previous result did not return to the first document.'
    Command 1172
    Command 1172
    Number (Control 105) 2024 4 | Out-Null
    [RstpdSmoke]::PostMessage((Control 105),0x100,[IntPtr]13,[IntPtr]::Zero) | Out-Null
    Wait-Until { (Caption $script:window) -match '^after.json' } 'Enter on a search result did not navigate to its document.'
    [RstpdSmoke]::PostMessage((Control 101),0x100,[IntPtr]115,[IntPtr]::Zero) | Out-Null
    Wait-Until { (Caption $script:window) -match '^before.json' } 'F4 did not wrap to the first result.'
    $resultPosition=Number (Control 105) 2167 4
    $resultX=(Number (Control 105) 2164 0 $resultPosition)+24
    $resultY=(Number (Control 105) 2165 0 $resultPosition)+5
    $click=($resultY -shl 16) -bor $resultX
    Number (Control 105) 0x201 1 $click | Out-Null
    Number (Control 105) 0x202 0 $click | Out-Null
    Number (Control 105) 0x201 1 $click | Out-Null
    Number (Control 105) 0x202 0 $click | Out-Null
    try {
        Wait-Until { (Caption $script:window) -match '^after.json' } 'Double-clicking a result did not navigate.'
    } catch {
        throw "$($_.Exception.Message) Window=$(Caption $script:window); Results=$(Caption (Control 305)); Click=$resultX,$resultY; Row=$(Number (Control 105) 2166 (Number (Control 105) 2008))."
    }
    $panelBefore=[RstpdSmoke+Rect]::new()
    [RstpdSmoke]::GetWindowRect((Control 105),[ref]$panelBefore) | Out-Null
    Number (Control 304) 0x201 1 0 | Out-Null
    Number (Control 304) 0x200 1 (30 -shl 16) | Out-Null
    Number (Control 304) 0x202 0 (30 -shl 16) | Out-Null
    Start-Sleep -Milliseconds 200
    $panelAfter=[RstpdSmoke+Rect]::new()
    [RstpdSmoke]::GetWindowRect((Control 105),[ref]$panelAfter) | Out-Null
    Assert (($panelAfter.bottom-$panelAfter.top) -lt ($panelBefore.bottom-$panelBefore.top)) 'The search-results panel cannot be resized.'
    foreach ($theme in @(1061,1062)) {
        Command $theme
        Check-SearchControls
        Screenshot $(if($theme -eq 1061){'smoke-light-results.png'}else{'smoke-dark-results.png'})
    }
    $originalRect=[RstpdSmoke+Rect]::new()
    [RstpdSmoke]::GetWindowRect($script:window,[ref]$originalRect) | Out-Null
    $dpi=[RstpdSmoke]::GetDpiForWindow($script:window)
    foreach ($width in @(780,1280)) {
        [RstpdSmoke]::MoveWindow($script:window,$originalRect.left,$originalRect.top,[int]($width*$dpi/96),[int](720*$dpi/96),$true) | Out-Null
        Start-Sleep -Milliseconds 250
        Check-SearchControls
        Screenshot "smoke-search-width-$width.png"
    }
    [RstpdSmoke]::MoveWindow($script:window,$originalRect.left,$originalRect.top,$originalRect.right-$originalRect.left,$originalRect.bottom-$originalRect.top,$true) | Out-Null
    Start-Sleep -Milliseconds 250
    Command 1040
    Screenshot 'smoke-search-focus.png'
    Number (Control 101) 2025 (Number (Control 101) 2006) | Out-Null
    Number (Control 101) 2078 | Out-Null
    Type-Text ' '
    Number (Control 101) 2079 | Out-Null
    Start-Sleep -Milliseconds 200
    $position=Number (Control 101) 2008
    Command 1172
    Command 1172
    Number (Control 105) 2024 4 | Out-Null
    [RstpdSmoke]::PostMessage((Control 105),0x100,[IntPtr]13,[IntPtr]::Zero) | Out-Null
    Wait-Until { (Caption (Control 305)) -match 'changed since the search' } 'Changed-document results were not rejected as stale.'
    Assert ((Number (Control 101) 2008) -eq $position) 'Stale-result navigation moved the editing caret.'
    Command 1014
    Assert ((Number (Control 101) 2008) -eq $position) 'An editing command from the read-only results panel changed the source.'
    Command 1176
    Command 1010
    Command 1171
    Wait-Until { (Caption (Control 305)) -match '^2 matches in 2 documents' } 'Find All could not be refreshed after editing.'
    Command 1173
    Command 1173
    [RstpdSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'missing-result-12345') | Out-Null
    Command 1170
    Wait-Until { (Caption (Control 305)) -match '^0 matches' } 'Find All did not report zero matches.'
    [RstpdSmoke]::PostMessage($script:window,0x111,[IntPtr]1171,[IntPtr]::Zero) | Out-Null
    [RstpdSmoke]::PostMessage($script:window,0x111,[IntPtr]1177,[IntPtr]::Zero) | Out-Null
    Wait-Until { (Caption (Control 305)) -match '^Search cancelled' } 'Find All cancellation did not discard its result.'
    Command 1175
    Assert ((Caption (Control 305)) -eq 'Search results cleared.') 'Clear did not reset search results.'
    Command 1176
    Assert (![RstpdSmoke]::IsWindowVisible((Control 105))) 'Close did not hide the search-results panel.'
    Command 1160
    Command 1304
    foreach ($button in @(
        @{ Id=1001; Name='New' },@{ Id=1002; Name='Open' },@{ Id=1003; Name='Save' },
        @{ Id=1040; Name='Find' },@{ Id=1050; Name='Split' },@{ Id=1070; Name='Compare' },
        @{ Id=1082; Name='JSON tree' },@{ Id=1051; Name='Document map' }
    )) {
        Assert ((Caption (Control $button.Id)) -eq $button.Name) 'An icon button lost its accessible name.'
        $rect=[RstpdSmoke+Rect]::new()
        [RstpdSmoke]::GetWindowRect((Control $button.Id),[ref]$rect) | Out-Null
        Assert ([Math]::Abs(($rect.right-$rect.left)-($rect.bottom-$rect.top)) -le 1) 'Toolbar buttons are not compact squares.'
    }
    $mnemonic = [RstpdSmoke]::SendMessage($script:window,0x120,[IntPtr][int][char]'f',[RstpdSmoke]::GetMenu($script:window)).ToInt64()
    Assert (($mnemonic -shr 16) -eq 2) 'Native menu keyboard mnemonic is not working.'
    Command 1070
    Start-Sleep -Milliseconds 700
    Assert ([RstpdSmoke]::IsWindowVisible((Control 102))) 'Compare did not open the second pane.'
    Assert ((Caption (Control 303)) -match 'Difference|difference') 'Compare did not finish.'
    Assert ((Number (Control 101) 2046 2) -ne 0) 'Compare did not mark a changed line.'
    $beforeLength = Number (Control 101) 2006
    Number (Control 101) 2025 $beforeLength | Out-Null
    Type-Text 'extra'
    $lastLine = Number (Control 101) 2166 (Number (Control 101) 2006)
    Wait-Until { (Number (Control 101) 2046 $lastLine) -ne 0 } 'Comparison did not refresh after editing.'
    Command 1010
    Wait-Until { (Number (Control 101) 2006) -eq $beforeLength } 'Live comparison interfered with undo.'
    Wait-Until { (Caption (Control 303)) -match 'difference groups|Difference' } 'Comparison did not refresh after undo.'
    Command 1062
    Screenshot 'smoke-dark-compare.png'
    Command 1073
    Command 1050
    Command 1082
    Wait-Until { (Number (Control 301) 0x1105) -gt 5 } 'JSON tree has no nodes.'
    Command 1051
    Assert ([RstpdSmoke]::IsWindowVisible((Control 104))) 'Document map did not open.'
    Command 1040
    [RstpdSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'value') | Out-Null
    [RstpdSmoke]::SendMessage((Control 402),0x0c,[IntPtr]::Zero,'score') | Out-Null
    Command 1044
    Number (Control 403) 0x14e 2 | Out-Null
    [RstpdSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'(?<=")(?P<key>score)(?=")') | Out-Null
    [RstpdSmoke]::SendMessage((Control 402),0x0c,[IntPtr]::Zero,'${key}_regex') | Out-Null
    Command 1044
    Number (Control 403) 0x14e 1 | Out-Null
    [RstpdSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'\x73core_regex') | Out-Null
    [RstpdSmoke]::SendMessage((Control 402),0x0c,[IntPtr]::Zero,'score') | Out-Null
    Command 1044
    Wait-Until { (Caption (Control 303)) -match 'Live JSON tree' } 'JSON tree did not refresh automatically.'
    $treeRoot = Number (Control 301) 0x110a 0
    $scoreNode = Number (Control 301) 0x110a 4 $treeRoot
    $scoreNode = Number (Control 301) 0x110a 1 $scoreNode
    $scoreNode = Number (Control 301) 0x110a 1 $scoreNode
    Number (Control 301) 0x110b 9 $scoreNode | Out-Null
    Wait-Until { (Caption (Control 303)) -match 'JSON pointer: /score' } 'Refreshed JSON navigation still uses stale data.'
    Screenshot 'smoke-dark-json.png'
    Command 1061
    Screenshot 'smoke-light-json.png'
    Command 1160
    Command 1091
    Command 5002
    Command 1003
    $saved = [IO.File]::ReadAllBytes((Join-Path $directory 'after.json'))
    Assert ($saved[0] -eq 255 -and $saved[1] -eq 254) 'UTF-16 LE conversion was not saved.'
    $savedText = [Text.Encoding]::Unicode.GetString($saved)
    Assert ($savedText.Contains('"score"') -and !$savedText.Contains('"score_regex"')) 'Regex or extended replacements failed.'
    Assert (!$savedText.Contains("`r")) 'LF conversion failed.'
    Command 1001
    foreach ($character in 'Recovered untitled'.ToCharArray()) {
        [RstpdSmoke]::SendMessage((Control 101),0x102,[IntPtr][int]$character,[IntPtr]::Zero) | Out-Null
    }
    Start-Sleep -Milliseconds 4000
    $session = Get-Content (Join-Path $directory 'session.json') -Raw | ConvertFrom-Json
    Assert ($session.documents.Count -eq 3) 'Recovery did not include all tabs.'
    Assert ($session.documents[-1].text -eq 'Recovered untitled') 'Untitled edits were not backed up.'
    Assert (($session.documents.text -join "`n") -match '"score"') 'Replacement was not persisted.'
    $memory = [math]::Round($script:process.WorkingSet64 / 1MB, 1)
    Stop-Process -Id $script:process.Id
    $script:process.WaitForExit()
    Start-Editor $false
    Assert ((Number (Control 302) 0x1304) -eq 3) 'Crash recovery lost tabs.'
    Assert ((Number (Control 101) 2006) -eq 'Recovered untitled'.Length) 'Crash recovery lost untitled text.'
    Close-Editor
    Start-Editor $false $false @('--import-language',(Join-Path $directory 'custom-language.xml'),(Join-Path $directory 'sample.rstlang'))
    Check-LanguageMenu $true
    $sayOffset = [IO.File]::ReadAllText((Join-Path $directory 'sample.rstlang')).IndexOf('say')
    Wait-Until { (Number (Control 101) 2010 $sayOffset) -eq 4 } 'Imported language did not highlight its keyword.'
    Assert (((Number (Control 101) 2223 0) -band 0x2000) -ne 0) 'Imported language folding was not applied.'
    Close-Editor
    Start-Editor $false $false @('--completion-api',(Join-Path $directory 'completion-api.xml'),(Join-Path $directory 'functions.rs'))
    Number (Control 101) 2025 (Number (Control 101) 2006) | Out-Null
    Command 1403
    Wait-Until { (Number (Control 101) 2202) -ne 0 } 'Document function parameter hints are not displayed.'
    Number (Control 101) 2013 | Out-Null
    Type-Text 'lookup_record('
    Wait-Until { (Number (Control 101) 2202) -ne 0 } 'Imported API parameter hints are not displayed.'
    Close-Editor
    Start-Editor $false
    Number (Control 101) 2025 (Number (Control 101) 2006) | Out-Null
    Command 1403
    Wait-Until { (Number (Control 101) 2202) -ne 0 } 'Imported API definitions were not restored.'
    Close-Editor
    Start-Editor $false $false @((Join-Path $directory 'sample.rstlang'))
    Wait-Until { (Number (Control 101) 2010 $sayOffset) -eq 4 } 'Imported language definitions were not restored.'
    Close-Editor
    Start-Editor $false $false @((Join-Path $directory 'highlighting.md'))
    $markdown = [IO.File]::ReadAllText((Join-Path $directory 'highlighting.md'))
    $heading = $markdown.IndexOf('visible heading')
    $body = $markdown.IndexOf('Plain body')
    $fence = $markdown.IndexOf('fn example')
    Wait-Until { (Number (Control 101) 2010 $heading) -eq 6 } 'Markdown headings were not styled by the application.'
    Assert ((Number (Control 101) 2010 $fence) -eq 21) 'Backtick code fences were not highlighted.'
    foreach ($theme in @(1061,1062)) {
        Command $theme
        $headingStyle = Number (Control 101) 2010 $heading
        $bodyStyle = Number (Control 101) 2010 $body
        Assert ((Number (Control 101) 2481 $headingStyle) -ne (Number (Control 101) 2481 $bodyStyle)) 'Markdown heading color matches plain text.'
        Assert ((Number (Control 101) 2483 $headingStyle) -ne 0) 'Markdown heading is not bold.'
        Screenshot $(if ($theme -eq 1061) {'smoke-light-markdown.png'} else {'smoke-dark-markdown.png'})
    }
    Command 1050
    Wait-Until { (Number (Control 102) 2010 $heading) -eq 6 } 'Split view lost Markdown highlighting.'
    Assert ((Number (Control 102) 2481 6) -ne (Number (Control 102) 2481 0)) 'Split view lost Markdown theme styling.'
    Command 1051
    Assert ((Number (Control 104) 2481 6) -ne (Number (Control 104) 2481 0)) 'Document map lost Markdown styling.'
    # Split moved the Markdown tab to the right pane; collapse it so the left editor shows it again.
    Command 1050
    Number (Control 101) 2025 (Number (Control 101) 2006) | Out-Null
    $ending = '**Live changes**'
    Type-Text "`r$ending"
    $liveOffset = (Number (Control 101) 2006) - $ending.Length + 2
    Wait-Until { (Number (Control 101) 2010 $liveOffset) -eq 2 } 'Markdown highlighting did not update after typing.'
    Close-Editor
    Start-Editor $false
    Wait-Until { (Number (Control 101) 2010 $liveOffset) -eq 2 } 'Restored Markdown document lost its highlighting.'
    Command 2000
    Assert ((Caption (Control 303)) -match '\|\s+Plain text\s+\|') 'Top-level Plain text command selected the wrong language.'
    $languageMenu=[RstpdSmoke]::GetSubMenu([RstpdSmoke]::GetMenu($script:window),5)
    $middle=[RstpdSmoke]::GetSubMenu($languageMenu,6)
    $foundMarkdown=$false
    for ($item=0; $item -lt [RstpdSmoke]::GetMenuItemCount($middle); $item++) {
        Command ([RstpdSmoke]::GetMenuItemID($middle,$item))
        if ((Caption (Control 303)) -match '\|\s+Markdown\s+\|') { $foundMarkdown=$true; break }
    }
    Assert $foundMarkdown 'Markdown is not selectable from the M-O language group.'
    Wait-Until { (Number (Control 101) 2010 $liveOffset) -eq 2 } 'Language menu selection did not restore Markdown highlighting.'
    Command 1001
    Assert ((Number (Control 101) 2020) -eq 0) 'Show All disabled whitespace did not apply to new tabs.'
    Command 1040
    [RstpdSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'^') | Out-Null
    Number (Control 403) 0x14e 2 | Out-Null
    Command 1170
    Wait-Until { (Caption (Control 305)) -match '^1 match in 1 document' } 'Find All did not include the empty untitled document.'
    Command 1173
    Assert ((Number (Control 101) 2143) -eq 0 -and (Number (Control 101) 2145) -eq 0) 'Zero-width result navigation is not exact.'
    Command 1005
    Command 1173
    Assert ((Caption (Control 305)) -match 'document was closed') 'Closed-tab result navigation did not report stale data.'
    Close-Editor
    Check-Workflows
    Check-LargeFileSearch
    Write-Host "PASS: Unicode/selection character counts, visible controls, symbols/preferences, Find All navigation, first launch, recovery and existing editor workflows. Working set: $memory MiB."
    Write-Host 'PASS: Drag/pin/recovery, queued new-tab double clicks, tab split/compare-current actions, external reload/conflict preservation, Find in Files/navigation, moved/selected/last-saved comparisons and non-editing alignment.'
    Write-Host 'Own-window captures: target\smoke-dark-compare.png, target\smoke-dark-json.png, target\smoke-light-json.png'
} finally {
    if ($script:process -and !$script:process.HasExited) {
        Stop-Process -Id $script:process.Id
        $script:process.WaitForExit()
    }
    foreach ($sessionDirectory in @($firstSessionDirectory,$legacySessionDirectory,$directory,$workflowSession,$workflowDirectory,$largeSession,$largeDirectory)) {
        foreach ($name in @('session.json','session.lock','before.json','after.json','custom-language.xml','sample.rstlang','completion-api.xml','functions.rs','highlighting.md','character-counts.txt','alpha.txt','beta.txt','disk-only.txt','large.log')) {
            $path = Join-Path $sessionDirectory $name
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
        }
    }
    foreach ($path in @($largeSession,$largeDirectory,$workflowSession,$workflowDirectory,$firstSessionDirectory,$legacySessionDirectory,$firstProfile,$directory)) {
        if ((Test-Path -LiteralPath $path) -and !(Get-ChildItem -LiteralPath $path -Force)) {
            Remove-Item -LiteralPath $path
        }
    }
    if ($originalDpiContext -ne [IntPtr]::Zero) {
        [RstpdSmoke]::SetThreadDpiAwarenessContext($originalDpiContext) | Out-Null
    }
}
