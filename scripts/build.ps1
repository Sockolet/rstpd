$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$originalPath = $env:PATH
Push-Location $root
try {
    if (!(Get-Command cargo -ErrorAction SilentlyContinue)) {
        $cargoDirectory = Join-Path $env:USERPROFILE '.cargo\bin'
        if (!(Test-Path (Join-Path $cargoDirectory 'cargo.exe'))) {
            throw 'Install Rust with the x86_64-pc-windows-msvc toolchain and Visual Studio C++ Build Tools.'
        }
        $env:PATH = "$cargoDirectory;$env:PATH"
    }
    & "$PSScriptRoot\bootstrap.ps1"
    if (!(Test-Path 'assets\rstpd.ico')) { & "$PSScriptRoot\generate-icon.ps1" }
    cargo test --locked --tests --quiet
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed.' }
    cargo build --locked --release
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $metadataText = cargo metadata --locked --offline --filter-platform x86_64-pc-windows-msvc --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Could not collect dependency license metadata.' }
    $metadata = $metadataText | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object { $_.name -eq 'rstpd' }).version
    $packageName = "rstpd-$version"
    $package = Join-Path $root "dist\$packageName"
    New-Item -ItemType Directory -Force $package | Out-Null
    Copy-Item 'target\release\rstpd.exe' $package
    Copy-Item 'README.md','LICENSE','THIRD_PARTY_NOTICES.txt' $package
    $licenses = Join-Path $package 'licenses'
    New-Item -ItemType Directory -Force $licenses | Out-Null
    Copy-Item 'vendor\scintilla\License.txt' (Join-Path $licenses 'Scintilla-Lexilla-SciTE.txt')
    $iconLicenses=Join-Path $licenses 'lucide-icons'
    New-Item -ItemType Directory -Force $iconLicenses | Out-Null
    Copy-Item 'assets\icons\LICENSE.txt','assets\icons\NOTICE.txt' $iconLicenses
    $sysroot = rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'Could not locate Rust runtime notices.' }
    $rustLicenses = Join-Path $licenses 'rust-standard-library'
    New-Item -ItemType Directory -Force $rustLicenses | Out-Null
    $runtimeNotice = Join-Path $sysroot 'share\doc\rust\COPYRIGHT-library.html'
    $runtimeLicenses = Join-Path $sysroot 'share\doc\rust\licenses'
    if (!(Test-Path $runtimeNotice) -or !(Test-Path $runtimeLicenses)) {
        throw 'Rust runtime notices are missing. Install the rust-docs toolchain component.'
    }
    Copy-Item $runtimeNotice $rustLicenses
    Copy-Item -LiteralPath $runtimeLicenses -Destination $rustLicenses -Recurse -Force
    $nodes = @{}
    foreach ($node in $metadata.resolve.nodes) { $nodes[$node.id] = $node }
    $reachable = [Collections.Generic.HashSet[string]]::new()
    $pending = [Collections.Generic.Queue[string]]::new()
    $pending.Enqueue($metadata.resolve.root)
    while ($pending.Count) {
        $id = $pending.Dequeue()
        if ($reachable.Add($id)) {
            foreach ($dependency in $nodes[$id].deps) { $pending.Enqueue($dependency.pkg) }
        }
    }
    $dependencies = @()
    foreach ($dependency in $metadata.packages) {
        if ($dependency.name -eq 'rstpd' -or !$reachable.Contains($dependency.id)) { continue }
        $dependencies += @{ name = $dependency.name; version = $dependency.version; license = $dependency.license }
        $source = Split-Path $dependency.manifest_path -Parent
        $notices = Get-ChildItem -LiteralPath $source -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE|UNLICENSE)([.-]|$)' }
        if ($notices) {
            $destination = Join-Path $licenses "$($dependency.name)-$($dependency.version)"
            New-Item -ItemType Directory -Force $destination | Out-Null
            $notices | Copy-Item -Destination $destination
        } else { throw "No redistribution notice found for $($dependency.name)." }
    }
    $dependencies | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $licenses 'dependencies.json') -Encoding utf8
    $archiveName = "$packageName-windows-x64.zip"
    $archive = Join-Path $root "dist\$archiveName"
    Compress-Archive -Path $package -DestinationPath $archive -Force
    $hash = (Get-FileHash $archive -Algorithm SHA256).Hash
    "$hash  $archiveName" | Set-Content "$archive.sha256" -Encoding ascii
    Write-Host "Portable app: $package\rstpd.exe"
    Write-Host "Distribution: $archive"
} finally {
    $env:PATH = $originalPath
    Pop-Location
}
