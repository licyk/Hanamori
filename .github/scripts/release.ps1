param(
    [Parameter(Mandatory)]
    [ValidateSet('Resolve', 'Verify', 'Package')]
    [string] $Stage,
    [string] $Tag = '',
    [ValidateSet('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')]
    [string] $Target = 'x86_64-pc-windows-msvc'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($Stage -eq 'Resolve' -and [string]::IsNullOrWhiteSpace($Tag)) {
    $tags = @(git tag --list 'v*' --sort=-version:refname)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot list release tags.' }
    if ($tags.Count -eq 0) { throw 'No existing v* release tag was found.' }
    $Tag = $tags[0]
}

# Values from workflow inputs are arguments, never interpolated PowerShell source.
if (-not $Tag.StartsWith('v', [StringComparison]::Ordinal)) {
    throw 'A release requires an existing v* tag; branch names are not release targets.'
}
git check-ref-format "refs/tags/$Tag"
if ($LASTEXITCODE -ne 0) { throw 'Invalid release tag.' }
$commit = git rev-parse --verify "refs/tags/$Tag^{commit}"
if ($LASTEXITCODE -ne 0) { throw 'The release tag must already exist and point to a commit.' }
if ($Stage -eq 'Resolve') {
    if ($env:GITHUB_OUTPUT) {
        "tag=$Tag" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
        "commit=$commit" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
    }
    Write-Host "Selected $Tag at $commit"
    return
}
$headCommit = git rev-parse --verify HEAD
if ($LASTEXITCODE -ne 0 -or $headCommit -ne $commit) {
    throw 'The checked-out commit does not match the release tag.'
}
git diff --quiet HEAD --
if ($LASTEXITCODE -ne 0) { throw 'Tracked source files differ from the release tag.' }
$metadataJson = cargo metadata --locked --no-deps --format-version 1
if ($LASTEXITCODE -ne 0) { throw 'Cannot read locked Cargo metadata.' }
$metadata = $metadataJson | ConvertFrom-Json
$package = @($metadata.packages | Where-Object { $_.name -eq 'hanamori' })
if ($package.Count -ne 1) { throw 'The tagged source does not contain the Hanamori package.' }
if ($Tag -cne "v$($package[0].version)") {
    throw "Release tag $Tag does not match Cargo package version $($package[0].version)."
}
$binaries = @($package[0].targets | Where-Object { $_.kind -contains 'bin' })
if ($binaries.Count -ne 1 -or $binaries[0].name -ne 'hanamori') {
    throw 'The tagged source must build only the Hanamori binary.'
}

if ($Stage -eq 'Verify') {
    if ($env:GITHUB_OUTPUT) {
        "tag=$Tag" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
        "commit=$commit" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
    }
    Write-Host "Verified $Tag at $commit (Hanamori $($package[0].version))"
    return
}

$binary = Join-Path $metadata.target_directory "$Target/release/hanamori.exe"
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "The $Target Hanamori release binary is missing."
}
# Check the executable architecture before assigning an architecture-specific asset name.
$bytes = [IO.File]::ReadAllBytes($binary)
if ($bytes.Length -lt 64 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
    throw 'The release binary is not a Windows PE executable.'
}
$machine = if ($Target -eq 'aarch64-pc-windows-msvc') { 0xaa64 } else { 0x8664 }
$peOffset = [BitConverter]::ToUInt32($bytes, 0x3c)
if ($peOffset -gt $bytes.Length - 6 -or
    [BitConverter]::ToUInt32($bytes, $peOffset) -ne 0x00004550 -or
    [BitConverter]::ToUInt16($bytes, $peOffset + 4) -ne $machine) {
    throw "The release binary does not match $Target."
}

$reportedVersion = & $binary --version
if ($LASTEXITCODE -ne 0 -or $reportedVersion -cne "hanamori $($package[0].version)") {
    throw 'The built executable does not report the release version.'
}

$asset = "hanamori-$Target.exe"
$archive = "hanamori-$Target.zip"
Copy-Item -LiteralPath $binary -Destination $asset -Force
Compress-Archive -LiteralPath $binary -DestinationPath $archive -Force
$sourceHash = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash
if ((Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash -ne $sourceHash) {
    throw 'The executable asset does not match the build output.'
}
$zip = [IO.Compression.ZipFile]::OpenRead((Join-Path (Get-Location) $archive))
try {
    if ($zip.Entries.Count -ne 1 -or $zip.Entries[0].FullName -cne 'hanamori.exe') {
        throw 'The release ZIP must contain only hanamori.exe at its root.'
    }
    $stream = $zip.Entries[0].Open()
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $archiveHash = [Convert]::ToHexString($sha.ComputeHash($stream))
        if ($archiveHash -ne $sourceHash) { throw 'The ZIP content differs from the build output.' }
    } finally {
        $sha.Dispose()
        $stream.Dispose()
    }
} finally {
    $zip.Dispose()
}
Write-Host "Packaged $Tag at $commit; executable SHA-256 $sourceHash"
