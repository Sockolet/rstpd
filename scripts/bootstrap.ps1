$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$packages = @(
    @{ Name = 'scintilla'; Archive = 'scintilla566.zip'; Hash = 'A0C0CDF1CF226DC6252020CE9A87A939A8B615E65269DB40AC9123B28FA20A9D' },
    @{ Name = 'lexilla'; Archive = 'lexilla553.zip'; Hash = '2092B1DD18355321717E3BDE25148E4C87E691723CA2B06A65E29A307C5462A6' }
)
foreach ($package in $packages) {
    $archive = Join-Path $root "vendor\$($package.Archive)"
    if (!(Test-Path $archive)) { throw "Missing vendored source archive: $archive" }
    if ((Get-FileHash $archive -Algorithm SHA256).Hash -ne $package.Hash) {
        throw "Source integrity check failed: $archive"
    }
    $destination = Join-Path $root "vendor\$($package.Name)"
    if (!(Test-Path $destination)) {
        Expand-Archive $archive -DestinationPath (Join-Path $root 'vendor')
    }
}
$dataArchive = Join-Path $root 'vendor\scite566.zip'
if ((Get-FileHash $dataArchive -Algorithm SHA256).Hash -ne '84D7DBE9D9CEB34961AD6FBDCF91A24F7CC1A2FD59D1851C0F5F4A0CDBFDE2F9') {
    throw 'Language data integrity check failed.'
}
$data = Join-Path $root 'vendor\language-data'
New-Item -ItemType Directory -Force $data | Out-Null
$zip = [IO.Compression.ZipFile]::OpenRead($dataArchive)
try {
    foreach ($entry in $zip.Entries) {
        if ($entry.FullName -match '^scite/(src/[^/]+\.properties|License\.txt)$') {
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $data $entry.Name), $true)
        }
    }
} finally { $zip.Dispose() }
Write-Host 'Pinned Scintilla and Lexilla sources are ready. Run cargo build --release.'
