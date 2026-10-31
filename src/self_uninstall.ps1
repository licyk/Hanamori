param([int]$ParentProcessId = 0)
$ErrorActionPreference = 'Stop'
try {
    if ($ParentProcessId -gt 0) {
        $parent = Get-Process -Id $ParentProcessId -ErrorAction SilentlyContinue
        if ($null -ne $parent) { $parent.WaitForExit() }
    }
    $files = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'files.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    foreach ($file in $files) {
        for ($attempt = 0; $attempt -lt 10; $attempt++) {
            try {
                if (Test-Path -LiteralPath $file) {
                    if (Test-Path -LiteralPath $file -PathType Container) { throw "Expected a file: $file" }
                    Remove-Item -LiteralPath $file -Force
                }
                break
            } catch {
                if ($attempt -eq 9) { throw }
                Start-Sleep -Milliseconds 250
            }
        }
    }
    Write-Output 'Hanamori executable removal completed.'
    foreach ($name in @('files.json', 'failure.txt', 'uninstall.ps1')) {
        $path = Join-Path $PSScriptRoot $name
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
    }
    [System.IO.Directory]::Delete($PSScriptRoot)
} catch {
    $message = "Hanamori self removal failed: $_`nClose processes using the files, then rerun: $PSCommandPath"
    $message | Set-Content -LiteralPath (Join-Path $PSScriptRoot 'failure.txt') -Encoding UTF8
    [Console]::Error.WriteLine($message)
    exit 1
}
