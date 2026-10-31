$ErrorActionPreference = 'Stop'
$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($env:INSTALLER_SOURCE, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($definition in $ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] }, $false)) {
    . ([scriptblock]::Create($definition.Extent.Text))
}
function Assert($condition, $message) {
    if (!$condition) { throw $message }
}
$root = $env:INSTALLER_ROOT
$location = $env:INSTALLER_REGISTRY
$key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($location)
$originalProcessPath = $env:PATH
$originalUserPath = '%SystemRoot%\installer-test;;D:\Keep'
$key.SetValue('Path', $originalUserPath, [Microsoft.Win32.RegistryValueKind]::ExpandString)
$options = @{
    InstallDir = Join-Path $root 'bin'
    ReleaseApi = $env:INSTALLER_API
    EnvironmentKey = $location
    ProfilePath = Join-Path $root 'profile.ps1'
}
try {
    [IO.File]::WriteAllText($options.ProfilePath, '# user profile', [Text.UTF8Encoding]::new($false))
    Install-Hanamori @options
    $executable = Join-Path $options.InstallDir 'hanamori.exe'
    $receipt = "$executable.install.json"
    Assert (Test-Path -LiteralPath $receipt) 'Missing PATH ownership receipt'
    Assert ((Get-Command hanamori).CommandType -eq 'Function') 'Current session hook was not loaded'
    Assert ((hanamori --version) -eq "hanamori $env:INSTALLER_VERSION") 'Hook did not invoke installed executable'
    Assert ($env:PATH.EndsWith($originalProcessPath)) 'Process PATH lost its existing entries'
    $userPath = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    Assert ($userPath -eq "$($options.InstallDir);$originalUserPath") 'User PATH changed unrelated entries'
    Assert ($key.GetValueKind('Path') -eq 'ExpandString') 'User PATH lost its registry type'
    $profile = [IO.File]::ReadAllText($options.ProfilePath)
    Assert ($profile.Contains('# user profile')) 'Profile content was lost'
    Assert ($profile.Contains('# >>> hanamori initialize >>>')) 'Missing persistent shell hook'
    $receiptBefore = [IO.File]::ReadAllText($receipt)
    Install-Hanamori @options -Version $env:INSTALLER_VERSION
    Assert ([IO.File]::ReadAllText($receipt) -eq $receiptBefore) 'Repeat installation changed PATH ownership'
    Assert ([IO.File]::ReadAllText($options.ProfilePath) -eq $profile) 'Repeat installation duplicated the hook'
    Assert ($key.GetValue('Path', '', 'DoNotExpandEnvironmentNames') -eq $userPath) 'Repeat installation duplicated PATH'
    $hash = (Get-FileHash -LiteralPath $executable).Hash
    foreach ($failure in @('bad-sha', 'bad-size', 'wrong-version', 'missing')) {
        $failedOptions = $options.Clone()
        $failedOptions.ReleaseApi = "$env:INSTALLER_API/$failure"
        $failed = $false
        try { Install-Hanamori @failedOptions } catch { $failed = $true }
        Assert $failed "Expected installation failure: $failure"
        Assert ((Get-FileHash -LiteralPath $executable).Hash -eq $hash) 'Failed download replaced the installed executable'
    }
    Assert (@(Get-ChildItem -LiteralPath $options.InstallDir -Directory).Count -eq 0) 'Staging files remained'
    Remove-Item Function:hanamori
    $bare = $options.Clone()
    $bare.InstallDir = Join-Path $root 'bare'
    $bare.ProfilePath = Join-Path $root 'unused-profile.ps1'
    $messages = @(Install-Hanamori @bare -NoShell -NoPathUpdate 3>&1)
    Assert (@($messages | Where-Object { $_ -is [Management.Automation.WarningRecord] -and $_.Message.Contains($executable) }).Count -gt 0) 'Existing executable was not reported'
    Assert (!(Test-Path -LiteralPath $bare.ProfilePath)) 'NoShell wrote a profile'
    Assert (!(Test-Path -LiteralPath (Join-Path $bare.InstallDir 'hanamori.exe.install.json'))) 'NoPathUpdate claimed ownership'
    Assert ($key.GetValue('Path', '', 'DoNotExpandEnvironmentNames') -eq $userPath) 'NoPathUpdate changed PATH'
    $preexisting = $options.Clone()
    $preexisting.InstallDir = Join-Path $root 'preexisting'
    $env:INSTALLER_EXISTING = $preexisting.InstallDir
    $key.SetValue('Path', "%INSTALLER_EXISTING%;$originalUserPath", [Microsoft.Win32.RegistryValueKind]::ExpandString)
    Install-Hanamori @preexisting -NoShell
    Assert (!(Test-Path -LiteralPath (Join-Path $preexisting.InstallDir 'hanamori.exe.install.json'))) 'Installer claimed a preexisting PATH entry'
    Assert ($key.GetValue('Path', '', 'DoNotExpandEnvironmentNames') -eq "%INSTALLER_EXISTING%;$originalUserPath") 'Preexisting expandable PATH entry was rewritten'
} finally {
    $key.Dispose()
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($location)
}
