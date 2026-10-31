param(
    [string]$Version = 'latest',
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Hanamori\bin'),
    [switch]$NoPathUpdate,
    [switch]$NoShell
)

function Install-Hanamori {
    param(
        [string]$Version = 'latest',
        [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Hanamori\bin'),
        [switch]$NoPathUpdate,
        [switch]$NoShell,
        [string]$ReleaseApi = 'https://api.github.com/repos/licyk/Hanamori/releases',
        [string]$EnvironmentKey = 'Environment',
        [string]$ProfilePath
    )

    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'
    $InstallDir = Resolve-HanamoriInstallDirectory $InstallDir
    $executable = Join-Path $InstallDir 'hanamori.exe'
    $oldTls = [Net.ServicePointManager]::SecurityProtocol
    $oldEncoding = [Console]::OutputEncoding
    try {
        [Net.ServicePointManager]::SecurityProtocol = $oldTls -bor [Net.SecurityProtocolType]::Tls12
        [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
        $release = Get-HanamoriRelease -Version $Version -ReleaseApi $ReleaseApi
        Install-HanamoriBinary -Release $release -Executable $executable
        Write-Host "Installed $executable"
        if (!$NoPathUpdate) {
            Add-HanamoriPath -InstallDir $InstallDir -EnvironmentKey $EnvironmentKey
        }
        $existing = Get-Command hanamori -ErrorAction SilentlyContinue
        if ($existing -and ($existing.CommandType -ne 'Application' -or $existing.Source -ne $executable)) {
            $description = if ($existing.CommandType -eq 'Application') { $existing.Source } else { "$($existing.Name) ($($existing.CommandType))" }
            Write-Warning "An existing command takes precedence: $description"
        }
        if (!$NoShell) {
            $shell = if ($PSVersionTable.PSEdition -eq 'Core') { 'pwsh.exe' } else { 'powershell.exe' }
            $startupArguments = @('shell', 'init', '--shell', 'powershell', '--completions')
            if ($ProfilePath) {
                $startupArguments += @('--profile', $ProfilePath)
            } else {
                $startupArguments += @('--shell-executable', (Join-Path $PSHOME $shell))
            }
            & $executable @startupArguments
            if ($LASTEXITCODE -ne 0) { throw 'Hanamori was installed, but shell initialization failed.' }
            $hook = & $executable env --shell powershell --completions
            if ($LASTEXITCODE -ne 0) { throw 'Hanamori was installed, but the current shell hook could not be generated.' }
            Invoke-Expression ($hook -join "`n")
            if ((Get-ExecutionPolicy) -in @('Restricted', 'AllSigned')) {
                Write-Warning 'Your execution policy may prevent the profile from loading in future terminals. No policy was changed.'
            }
        }
        Write-Host 'Ready. Install a toolchain when needed: hanamori install latest --name stable'
        Write-Host 'Updates: hanamori upgrade | Removal: hanamori self uninstall'
        Write-Host 'If this installer ran in a child process, open a new terminal to load PATH and the shell hook.'
    } finally {
        [Net.ServicePointManager]::SecurityProtocol = $oldTls
        [Console]::OutputEncoding = $oldEncoding
    }
}

function Resolve-HanamoriInstallDirectory {
    param([string]$Directory)

    if ([Environment]::OSVersion.Platform -ne 'Win32NT') {
        throw 'Hanamori requires Windows x64.'
    }
    $architecture = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment').PROCESSOR_ARCHITECTURE
    if ($architecture -ne 'AMD64') { throw 'Hanamori requires Windows x64.' }
    $directory = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Directory)
    if ($directory.Contains(';')) { throw 'The installation directory cannot contain a semicolon (PATH separator).' }
    $directory
}

function Install-HanamoriBinary {
    param([pscustomobject]$Release, [string]$Executable)

    $staging = Join-Path (Split-Path -Parent $Executable) ('.hanamori-install-' + [Guid]::NewGuid().ToString('N'))
    $null = [IO.Directory]::CreateDirectory($staging)
    try {
        $download = Join-Path $staging 'hanamori.exe'
        Write-Host "Downloading Hanamori $($Release.Version)..."
        Invoke-WebRequest -UseBasicParsing -Uri $Release.Url -OutFile $download -TimeoutSec 180
        if ((Get-Item -LiteralPath $download).Length -ne $Release.Size) {
            throw 'Downloaded executable size differs from the release asset.'
        }
        if ((Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash -ne $Release.Sha256) {
            throw 'Downloaded executable SHA-256 differs from the release asset.'
        }
        $reportedVersion = & $download --version
        if ($LASTEXITCODE -ne 0 -or "$reportedVersion".Trim() -cne "hanamori $($Release.Version)") {
            throw 'Downloaded executable did not report the expected Hanamori version.'
        }
        if (Test-Path -LiteralPath $Executable) {
            # PowerShell converts $null to an empty string for this .NET string argument.
            [IO.File]::Replace($download, $Executable, [NullString]::Value)
        } else {
            [IO.File]::Move($download, $Executable)
        }
    } finally {
        Remove-Item -LiteralPath $staging -Recurse -Force
    }
}

function Get-HanamoriRelease {
    param([string]$Version, [string]$ReleaseApi)

    $suffix = 'latest'
    if ($Version -ne 'latest') {
        $Version = $Version -creplace '^v', ''
        if ($Version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
            throw 'Version must be latest or a release version such as 0.1.0.'
        }
        $suffix = 'tags/' + [Uri]::EscapeDataString("v$Version")
    }
    $headers = @{ Accept = 'application/vnd.github+json'; 'User-Agent' = 'Hanamori-Installer' }
    if ($env:GITHUB_TOKEN) { $headers.Authorization = "Bearer $env:GITHUB_TOKEN" }
    $metadata = Invoke-RestMethod -Uri "$ReleaseApi/$suffix" -Headers $headers -TimeoutSec 60
    $tag = [string]$metadata.tag_name
    if ($metadata.draft -or ($Version -eq 'latest' -and $metadata.prerelease) -or
        $tag -cnotmatch '^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
        throw 'The release has no usable Hanamori version.'
    }
    if ($Version -ne 'latest' -and $tag -cne "v$Version") { throw 'The release tag differs from the requested version.' }
    $assets = @($metadata.assets | Where-Object { $_.name -eq 'hanamori-x86_64-pc-windows-msvc.exe' })
    if ($assets.Count -ne 1) { throw 'The release must contain one Windows x64 Hanamori executable.' }
    $asset = $assets[0]
    if ($asset.size -le 0 -or $asset.digest -notmatch '^sha256:([0-9a-fA-F]{64})$' -or !$asset.browser_download_url) {
        throw 'The release asset must provide a download URL, size and SHA-256 digest.'
    }
    [pscustomobject]@{
        Version = $tag.Substring(1)
        Url = $asset.browser_download_url
        Size = $asset.size
        Sha256 = $asset.digest.Substring(7)
    }
}

function Add-HanamoriPath {
    param([string]$InstallDir, [string]$EnvironmentKey)

    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($EnvironmentKey)
    try {
        $path = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
        if ($key.GetValueNames() -contains 'Path') { $kind = $key.GetValueKind('Path') }
        $entries = @($path -split ';' | Where-Object {
            [Environment]::ExpandEnvironmentVariables($_).Trim('"').TrimEnd('\') -ieq $InstallDir.TrimEnd('\')
        })
        if ($entries.Count -eq 0) {
            $receipt = Join-Path $InstallDir 'hanamori.exe.install.json'
            $json = @{ path_entry = $InstallDir } | ConvertTo-Json -Compress
            [IO.File]::WriteAllText($receipt, $json, [Text.UTF8Encoding]::new($false))
            $updated = if ($path) { "$InstallDir;$path" } else { $InstallDir }
            $key.SetValue('Path', $updated, $kind)
            Publish-HanamoriEnvironment
        }
    } finally {
        $key.Dispose()
    }
    if (@($env:PATH -split ';' | Where-Object { $_.Trim('"').TrimEnd('\') -ieq $InstallDir.TrimEnd('\') }).Count -eq 0) {
        $env:PATH = "$InstallDir;$env:PATH"
    }
}

function Publish-HanamoriEnvironment {
    if (!('Hanamori.InstallerEnvironment' -as [type])) {
        Add-Type @'
using System;
using System.Runtime.InteropServices;
namespace Hanamori {
    public static class InstallerEnvironment {
        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern IntPtr SendMessageTimeout(IntPtr window, uint message,
            UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);
    }
}
'@
    }
    $result = [UIntPtr]::Zero
    $null = [Hanamori.InstallerEnvironment]::SendMessageTimeout([IntPtr]0xffff, 0x1a,
        [UIntPtr]::Zero, 'Environment', 2, 1000, [ref]$result)
}

Install-Hanamori @PSBoundParameters
