$ErrorActionPreference = 'Stop'
$location = $env:INSTALLER_REGISTRY
$key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($location)
try {
    $key.SetValue('Path', 'D:\Keep')
    $script = (Join-Path $PSScriptRoot 'bash-install.sh').Replace('\', '/')
    & $env:INSTALLER_BASH --noprofile --norc $script
    if ($LASTEXITCODE -ne 0) { throw "Bash installer test failed: $LASTEXITCODE" }
    $path = $key.GetValue('Path')
    if (!$path.EndsWith(';D:\Keep')) { throw "Unrelated PATH entry was lost: $path" }
    if (@($path -split ';').Count -ne 2) { throw "Duplicate PATH entry: $path" }
} finally {
    $key.Dispose()
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($location)
}
