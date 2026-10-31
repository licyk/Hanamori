use crate::shell::syntax::Shell;

pub(super) fn registration(shell: Shell, enabled: bool) -> String {
    match shell {
        Shell::Bash | Shell::Zsh => {
            let mut script = String::new();
            if shell == Shell::Bash {
                script.push_str(
                    r#"if declare -F _hanamori_auto_use >/dev/null; then
    _hanamori_prompts=()
    for _hanamori_prompt in "${PROMPT_COMMAND[@]}"; do
        if [ "$_hanamori_prompt" != _hanamori_auto_use ]; then
            _hanamori_prompts+=("$_hanamori_prompt")
        fi
    done
    PROMPT_COMMAND=("${_hanamori_prompts[@]}")
    unset _hanamori_prompts _hanamori_prompt
fi
"#,
                );
            } else {
                script.push_str(
                    "autoload -Uz add-zsh-hook\nadd-zsh-hook -d chpwd _hanamori_auto_use\n",
                );
            }
            if !enabled {
                script.push_str("unset -f _hanamori_auto_use 2>/dev/null || :\nunset _hanamori_last_directory\n");
                return script;
            }
            script.push_str(
                r#"_hanamori_auto_use() {
    local _hanamori_status=$?
    if [ "${_hanamori_last_directory-}" != "$PWD" ]; then
        _hanamori_last_directory=$PWD
        if ! hanamori use --automatic; then :; fi
    fi
    return "$_hanamori_status"
}
"#,
            );
            if shell == Shell::Bash {
                script.push_str("PROMPT_COMMAND=(_hanamori_auto_use \"${PROMPT_COMMAND[@]}\")\n");
            } else {
                script.push_str("add-zsh-hook chpwd _hanamori_auto_use\n");
            }
            script.push_str("_hanamori_auto_use || :\n");
            script
        }
        Shell::PowerShell => {
            if !enabled {
                return r#"if (Get-Variable _hanamori_original_prompt -Scope Global -ErrorAction Ignore) {
    Set-Item Function:global:prompt $global:_hanamori_original_prompt
    Remove-Variable _hanamori_original_prompt -Scope Global
    Remove-Item Function:_hanamori_auto_use -ErrorAction Ignore
    Remove-Variable _hanamori_last_directory -Scope Global -ErrorAction Ignore
}
"#.into();
            }
            r#"if (!(Get-Variable _hanamori_original_prompt -Scope Global -ErrorAction Ignore)) {
    $global:_hanamori_original_prompt = (Get-Item Function:prompt).ScriptBlock
    $global:_hanamori_last_directory = $null
}
function global:_hanamori_auto_use {
    $location = Get-Location
    if ($location.Provider.Name -ne 'FileSystem') { return }
    if ($global:_hanamori_last_directory -cne $location.ProviderPath) {
        $global:_hanamori_last_directory = $location.ProviderPath
        hanamori use --automatic
    }
}
function global:prompt {
    $previousSuccess = $?
    $previousExit = Get-Variable LASTEXITCODE -Scope Global -ErrorAction Ignore
    $previousExitValue = if ($previousExit) { $previousExit.Value } else { $null }
    try { _hanamori_auto_use } finally {
        if ($previousExit) { $global:LASTEXITCODE = $previousExitValue }
        else { Remove-Variable LASTEXITCODE -Scope Global -ErrorAction Ignore }
    }
    if (!$previousSuccess) { Write-Error 'Previous command failed' -ErrorAction Ignore }
    . $global:_hanamori_original_prompt
}
& {
    $previousExit = Get-Variable LASTEXITCODE -Scope Global -ErrorAction Ignore
    $previousExitValue = if ($previousExit) { $previousExit.Value } else { $null }
    try { _hanamori_auto_use } finally {
        if ($previousExit) { $global:LASTEXITCODE = $previousExitValue }
        else { Remove-Variable LASTEXITCODE -Scope Global -ErrorAction Ignore }
    }
}
"#
            .into()
        }
        Shell::Cmd => unreachable!("CMD uses persistent DOSKEY registration"),
    }
}
