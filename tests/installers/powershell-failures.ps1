$ErrorActionPreference = 'Stop'
$tokens = $null
$errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($env:INSTALLER_SOURCE, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($definition in $ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] }, $false)) {
    . ([scriptblock]::Create($definition.Extent.Text))
}
$executable = Join-Path $env:INSTALLER_ROOT 'hanamori.exe'
[IO.File]::Copy($env:INSTALLER_BINARY, $executable)
$hash = (Get-FileHash -LiteralPath $executable).Hash
foreach ($scenario in @(
    @{Route = 'bad-sha'; Error = 'SHA-256'},
    @{Route = 'bad-size'; Error = 'size differs'},
    @{Route = 'wrong-version'; Error = 'expected Hanamori version'},
    @{Route = 'missing'; Error = '404'}
)) {
    $release = Get-HanamoriRelease -Version latest -ReleaseApi "$env:INSTALLER_API/$($scenario.Route)"
    $failed = $false
    try { Install-HanamoriBinary -Release $release -Executable $executable } catch {
        if (!$_.ToString().Contains($scenario.Error)) { throw }
        $failed = $true
    }
    if (!$failed) { throw "Expected installation failure: $($scenario.Route)" }
    if ((Get-FileHash -LiteralPath $executable).Hash -ne $hash) { throw 'Failed download replaced the installed executable' }
    if (@(Get-ChildItem -LiteralPath $env:INSTALLER_ROOT -Directory).Count) { throw 'Staging files remained' }
}
