    $hanamoriResults = @(& {
        $words = [Collections.Generic.List[string]]::new()
        $hasCurrent = $false
        foreach ($element in $commandAst.CommandElements) {
            if ($element.Extent.StartOffset -ge $cursorPosition) { break }
            if ($element -isnot [ConstantExpressionAst] -and
                $element -isnot [CommandParameterAst]) { return }
            if ($element.Extent.EndOffset -ge $cursorPosition) {
                $words.Add($element.Extent.Text.Substring(0, $cursorPosition - $element.Extent.StartOffset).Trim("'", '"'))
                $hasCurrent = $true
                break
            }
            if ($element -is [StringConstantExpressionAst]) { $words.Add($element.Value) }
            else { $words.Add($element.Extent.Text) }
        }
        if (!$hasCurrent) { $words.Add('') }
        $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes((ConvertTo-Json -InputObject $words.ToArray() -Compress)))
        $exitCode = Get-Variable -Name LASTEXITCODE -Scope Global -ErrorAction SilentlyContinue
        $savedExitCode = if ($null -ne $exitCode) { $exitCode.Value } else { $null }
        try {
            & @PROGRAM@ shell complete --encoded-args $encoded 2>$null
        } catch {
        } finally {
            if ($null -ne $exitCode) { $global:LASTEXITCODE = $savedExitCode }
            else { Remove-Variable -Name LASTEXITCODE -Scope Global -ErrorAction SilentlyContinue }
        }
    })
    if ($hanamoriResults.Count -gt 0 -and $hanamoriResults[0] -eq '@hanamori') {
        for ($i = 1; $i -lt $hanamoriResults.Count; $i++) {
            $value = $hanamoriResults[$i]
            $type = if ($value.StartsWith('-')) { [CompletionResultType]::ParameterName } else { [CompletionResultType]::ParameterValue }
            [CompletionResult]::new($value, $value, $type, $value)
        }
        return
    }
