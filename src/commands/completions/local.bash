_hanamori_local() {
    local _word _reply _value _join=0 _trim=''
    local -a _words=()
    # Readline splits --option=value at '='; restore argv for Clap, then remove
    # the option prefix from candidates when Readline replaces only the value.
    for _word in "${COMP_WORDS[@]:0:COMP_CWORD+1}"; do
        if [[ $_word == = && ${#_words[@]} -gt 1 && ${_words[${#_words[@]}-1]} == --* ]]; then
            _words[${#_words[@]}-1]+='='
            _join=1
        elif [[ $_join == 1 ]]; then
            _words[${#_words[@]}-1]+=$_word
            _join=0
        else
            _words+=("$_word")
        fi
    done
    if [[ ${_words[${#_words[@]}-1]} != "${COMP_WORDS[COMP_CWORD]}" ]]; then
        _trim=${_words[${#_words[@]}-1]%%=*}=
    fi
    if ! _reply=$(command @PROGRAM@ shell complete --raw-bash -- "${_words[@]}" 2>/dev/null); then return 1; fi
    [[ ${_reply%%$'\n'*} == @hanamori ]] || return 1
    COMPREPLY=()
    while IFS= read -r _value; do
        [[ $_value == @hanamori ]] && continue
        COMPREPLY+=("${_value#"$_trim"}")
    done <<< "$_reply"
    return 0
}
