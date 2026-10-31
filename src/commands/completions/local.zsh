_hanamori_local() {
    local _reply
    local -a _words _values
    _words=("${(@Q)words[1,CURRENT]}")
    _words[CURRENT]=${(Q)PREFIX}
    if ! _reply=$(command @PROGRAM@ shell complete -- "${_words[@]}" 2>/dev/null); then return 1; fi
    _values=("${(@f)_reply}")
    [[ ${_values[1]-} == @hanamori ]] || return 1
    shift _values
    if (( ${#_values} )); then compadd -a _values; fi
    return 0
}
