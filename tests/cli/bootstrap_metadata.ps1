$ErrorActionPreference = 'Stop'
$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($env:INSTALLER_SOURCE, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($definition in $ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] }, $false)) {
    . ([scriptblock]::Create($definition.Extent.Text))
}
$release = Get-HanamoriRelease -Version latest -ReleaseApi $env:INSTALLER_API
if ($release.Version -ne '1.2.3') { throw 'Unexpected release version' }
$release = Get-HanamoriRelease -Version v1.2.3 -ReleaseApi $env:INSTALLER_API
if ($release.Version -ne '1.2.3') { throw 'Explicit version was not resolved' }
foreach ($scenario in @(
    @{Version = 'latest'; ReleaseApi = "$env:INSTALLER_API/prerelease"},
    @{Version = 'latest'; ReleaseApi = "$env:INSTALLER_API/no-digest"},
    @{Version = '2.0.0'; ReleaseApi = $env:INSTALLER_API},
    @{Version = 'invalid'; ReleaseApi = $env:INSTALLER_API}
)) {
    $failed = $false
    try { $null = Get-HanamoriRelease @scenario } catch { $failed = $true }
    if (!$failed) { throw "Invalid release accepted: $($scenario | ConvertTo-Json -Compress)" }
}
$release = Get-HanamoriRelease -Version latest -ReleaseApi "$env:INSTALLER_API/bad-sha"
$executable = Join-Path $env:INSTALLER_ROOT 'hanamori.exe'
[IO.File]::WriteAllText($executable, 'original executable')
$failed = $false
try { Install-HanamoriBinary -Release $release -Executable $executable } catch {
    if (!$_.ToString().Contains('SHA-256')) { throw }
    $failed = $true
}
if (!$failed) { throw 'Corrupt download was accepted' }
if ([IO.File]::ReadAllText($executable) -ne 'original executable') { throw 'Existing executable changed' }
if (@(Get-ChildItem -LiteralPath $env:INSTALLER_ROOT -Directory).Count) { throw 'Staging directory remained' }
