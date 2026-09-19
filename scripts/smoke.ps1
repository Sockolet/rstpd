param([string]$Executable = "$PSScriptRoot\..\target\debug\rstpad.exe")
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class RstpadSmoke {
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h, int id);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, string l);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetMenu(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetSubMenu(IntPtr menu, int index);
    [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] public static extern uint GetMenuItemID(IntPtr menu, int index);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder b, int count);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out Rect rect);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left, top, right, bottom; }
}
'@
$root = Split-Path $PSScriptRoot -Parent
$directory = Join-Path $root "target\smoke-$([Guid]::NewGuid().ToString('N'))"
$firstProfile = Join-Path $directory 'fresh-profile'
$firstSessionDirectory = Join-Path $firstProfile 'RSTPad'
$firstSession = Join-Path $firstSessionDirectory 'session.json'
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
}
function Close-Editor {
    [RstpadSmoke]::PostMessage($script:window,0x10,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
    Assert ($script:process.WaitForExit(10000)) 'App did not flush and close.'
    Assert ($script:process.ExitCode -eq 0) "App exited with code $($script:process.ExitCode)."
}
function Command([int]$id) {
    $button=Control $id
    if ($button -ne [IntPtr]::Zero -and $id -in @(1001,1002,1003,1040,1050,1051,1070,1082)) {
        [RstpadSmoke]::PostMessage($button,0xF5,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
    } else {
        [RstpadSmoke]::PostMessage($script:window,0x111,[IntPtr]$id,[IntPtr]::Zero) | Out-Null
    }
    Start-Sleep -Milliseconds 200
}
function Control([int]$id) { [RstpadSmoke]::GetDlgItem($script:window,$id) }
function Number([IntPtr]$handle,[uint32]$message,[long]$w = 0,[long]$l = 0) {
    [RstpadSmoke]::SendMessage($handle,$message,[IntPtr]$w,[IntPtr]$l).ToInt64()
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
function Type-Text([string]$text) {
    foreach ($character in $text.ToCharArray()) {
        [RstpadSmoke]::SendMessage((Control 101),0x102,[IntPtr][int]$character,[IntPtr]::Zero) | Out-Null
    }
}
function Caption([IntPtr]$handle) {
    $text = [Text.StringBuilder]::new(4096)
    [RstpadSmoke]::GetWindowText($handle,$text,$text.Capacity) | Out-Null
    $text.ToString()
}
function Check-LanguageMenu([bool]$custom) {
    $menu=[RstpadSmoke]::GetSubMenu([RstpadSmoke]::GetMenu($script:window),5)
    Assert ($menu -ne [IntPtr]::Zero) 'Language menu is missing.'
    Assert ([RstpadSmoke]::GetMenuItemID($menu,0) -eq 2000) 'Plain text is not directly accessible.'
    $count=[RstpadSmoke]::GetMenuItemCount($menu)
    Assert ($count -eq $(if ($custom) {15} else {14})) 'Language menu contains unexpected top-level categories.'
    $letters=@('a','d','g','j','m','p','s','v')
    $ids=[Collections.Generic.List[uint32]]::new()
    $ids.Add(2000)
    for ($index=0; $index -lt $letters.Count; $index++) {
        $mnemonic=[RstpadSmoke]::SendMessage($script:window,0x120,[IntPtr][int][char]$letters[$index],$menu).ToInt64()
        Assert (($mnemonic -shr 16) -eq 2 -and ($mnemonic -band 0xffff) -eq ($index+2)) 'Alphabetical language group has an incorrect label or mnemonic.'
        $group=[RstpadSmoke]::GetSubMenu($menu,$index+2)
        Assert ($group -ne [IntPtr]::Zero) 'Language letter group is missing.'
        for ($item=0; $item -lt [RstpadSmoke]::GetMenuItemCount($group); $item++) {
            $ids.Add([RstpadSmoke]::GetMenuItemID($group,$item))
        }
    }
    if ($custom) {
        $group=[RstpadSmoke]::GetSubMenu($menu,10)
        Assert ($group -ne [IntPtr]::Zero) 'User-defined languages are not separate.'
        for ($item=0; $item -lt [RstpadSmoke]::GetMenuItemCount($group); $item++) {
            $ids.Add([RstpadSmoke]::GetMenuItemID($group,$item))
        }
    }
    $sorted=@($ids | Sort-Object)
    Assert ($sorted.Count -ge 100) 'Languages were lost during menu regrouping.'
    for ($index=0; $index -lt $sorted.Count; $index++) {
        Assert ($sorted[$index] -eq 2000+$index) 'A language command is missing, duplicated or misnumbered.'
    }
    Assert ([RstpadSmoke]::GetMenuItemID($menu,$count-3) -eq 1400) 'Language management commands are not at the bottom.'
}
function Screenshot([string]$name) {
    $previous = [RstpadSmoke]::SetThreadDpiAwarenessContext([IntPtr](-4))
    $renderers = @()
    try {
        # DirectWrite capture can be blank on an inactive/remote desktop. Use GDI only for the capture.
        foreach ($id in @(101,102,104)) {
            $control = Control $id
            $renderers += @{ Handle=$control; Technology=(Number $control 2631) }
            Number $control 2630 0 | Out-Null
        }
        $rect = [RstpadSmoke+Rect]::new()
        [RstpadSmoke]::GetWindowRect($script:window,[ref]$rect) | Out-Null
        $bitmap = [Drawing.Bitmap]::new($rect.right-$rect.left,$rect.bottom-$rect.top)
        try {
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            $dc = $graphics.GetHdc()
            try { Assert ([RstpadSmoke]::PrintWindow($script:window,$dc,2)) 'Window capture failed.' }
            finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
            $bitmap.Save((Join-Path $root "target\$name"),[Drawing.Imaging.ImageFormat]::Png)
        } finally { $bitmap.Dispose() }
    } finally {
        foreach ($renderer in $renderers) { Number $renderer.Handle 2630 $renderer.Technology | Out-Null }
        if ($previous -ne [IntPtr]::Zero) { [RstpadSmoke]::SetThreadDpiAwarenessContext($previous) | Out-Null }
    }
}
try {
    Start-Editor $false $true
    Assert ((Number (Control 302) 0x1304) -eq 1) 'First launch must create one untitled tab.'
    Assert ((Number (Control 101) 2006) -eq 0) 'First-launch document must be empty.'
    foreach ($character in 'First-launch text'.ToCharArray()) {
        [RstpadSmoke]::SendMessage((Control 101),0x102,[IntPtr][int]$character,[IntPtr]::Zero) | Out-Null
    }
    Close-Editor
    $firstRecovery = Get-Content -LiteralPath $firstSession -Raw | ConvertFrom-Json
    Assert ($firstRecovery.documents.Count -eq 1) 'First-launch recovery lost the untitled tab.'
    Assert ($null -eq $firstRecovery.documents[0].path) 'First-launch tab must not require a filename.'
    Assert ($firstRecovery.documents[0].text -eq 'First-launch text') 'First-launch editing or recovery failed.'

    Start-Editor $false $true
    Assert ((Number (Control 302) 0x1304) -eq 1) 'No-argument relaunch must restore the untitled tab.'
    Assert ((Number (Control 101) 2006) -eq 'First-launch text'.Length) 'No-argument relaunch lost recovered text.'
    Close-Editor

    @{ version = 1; documents = @(); active = 0; theme = 'system' } |
        ConvertTo-Json | Set-Content -LiteralPath $firstSession -Encoding utf8
    Start-Editor $false $true
    Assert ((Number (Control 302) 0x1304) -eq 1) 'An empty saved session must create an untitled tab.'
    Assert ((Number (Control 101) 2006) -eq 0) 'Empty-session startup must create a blank document.'
    Close-Editor

    Start-Editor $true
    Assert ((Number (Control 302) 0x1304) -eq 2) 'Expected two file tabs.'
    Check-LanguageMenu $false
    foreach ($button in @(
        @{ Id=1001; Name='New' },@{ Id=1002; Name='Open' },@{ Id=1003; Name='Save' },
        @{ Id=1040; Name='Find' },@{ Id=1050; Name='Split' },@{ Id=1070; Name='Compare' },
        @{ Id=1082; Name='JSON tree' },@{ Id=1051; Name='Document map' }
    )) {
        Assert ((Caption (Control $button.Id)) -eq $button.Name) 'An icon button lost its accessible name.'
        $rect=[RstpadSmoke+Rect]::new()
        [RstpadSmoke]::GetWindowRect((Control $button.Id),[ref]$rect) | Out-Null
        Assert ([Math]::Abs(($rect.right-$rect.left)-($rect.bottom-$rect.top)) -le 1) 'Toolbar buttons are not compact squares.'
    }
    $mnemonic = [RstpadSmoke]::SendMessage($script:window,0x120,[IntPtr][int][char]'f',[RstpadSmoke]::GetMenu($script:window)).ToInt64()
    Assert (($mnemonic -shr 16) -eq 2) 'Native menu keyboard mnemonic is not working.'
    Command 1070
    Start-Sleep -Milliseconds 700
    Assert ([RstpadSmoke]::IsWindowVisible((Control 102))) 'Compare did not open the second pane.'
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
    Assert ([RstpadSmoke]::IsWindowVisible((Control 104))) 'Document map did not open.'
    Command 1040
    [RstpadSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'value') | Out-Null
    [RstpadSmoke]::SendMessage((Control 402),0x0c,[IntPtr]::Zero,'score') | Out-Null
    Command 1044
    Number (Control 403) 0x14e 2 | Out-Null
    [RstpadSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'(?<=")(?P<key>score)(?=")') | Out-Null
    [RstpadSmoke]::SendMessage((Control 402),0x0c,[IntPtr]::Zero,'${key}_regex') | Out-Null
    Command 1044
    Number (Control 403) 0x14e 1 | Out-Null
    [RstpadSmoke]::SendMessage((Control 401),0x0c,[IntPtr]::Zero,'\x73core_regex') | Out-Null
    [RstpadSmoke]::SendMessage((Control 402),0x0c,[IntPtr]::Zero,'score') | Out-Null
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
        [RstpadSmoke]::SendMessage((Control 101),0x102,[IntPtr][int]$character,[IntPtr]::Zero) | Out-Null
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
    $languageMenu=[RstpadSmoke]::GetSubMenu([RstpadSmoke]::GetMenu($script:window),5)
    $middle=[RstpadSmoke]::GetSubMenu($languageMenu,6)
    $foundMarkdown=$false
    for ($item=0; $item -lt [RstpadSmoke]::GetMenuItemCount($middle); $item++) {
        Command ([RstpadSmoke]::GetMenuItemID($middle,$item))
        if ((Caption (Control 303)) -match '\|\s+Markdown\s+\|') { $foundMarkdown=$true; break }
    }
    Assert $foundMarkdown 'Markdown is not selectable from the M-O language group.'
    Wait-Until { (Number (Control 101) 2010 $liveOffset) -eq 2 } 'Language menu selection did not restore Markdown highlighting.'
    Close-Editor
    Write-Host "PASS: language-menu grouping/selection, icon-button actions, first launch, recovery, live tools, regex, encodings, completion, UDL/API persistence and Markdown styling. Working set: $memory MiB."
    Write-Host 'Own-window captures: target\smoke-dark-compare.png, target\smoke-dark-json.png, target\smoke-light-json.png'
} finally {
    if ($script:process -and !$script:process.HasExited) {
        Stop-Process -Id $script:process.Id
        $script:process.WaitForExit()
    }
    foreach ($sessionDirectory in @($firstSessionDirectory,$directory)) {
        foreach ($name in @('session.json','session.lock','before.json','after.json','custom-language.xml','sample.rstlang','completion-api.xml','functions.rs','highlighting.md')) {
            $path = Join-Path $sessionDirectory $name
            if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
        }
    }
    foreach ($path in @($firstSessionDirectory,$firstProfile,$directory)) {
        if ((Test-Path -LiteralPath $path) -and !(Get-ChildItem -LiteralPath $path -Force)) {
            Remove-Item -LiteralPath $path
        }
    }
}
