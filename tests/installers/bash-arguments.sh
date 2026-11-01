set -euo pipefail
installer=$INSTALLER_SOURCE
if [[ ${OS:-} == Windows_NT ]]; then
    installer=$(cygpath -au -- "$installer")
fi

reject() {
    if "$BASH" "$installer" "$@"; then
        printf 'Invalid arguments were accepted: %s\n' "$*" >&2
        exit 1
    fi
}

"$BASH" "$installer" --help | grep -q 'MSYS2 or Git Bash'
reject --unknown
reject --version
reject --install-dir ''
case "$(uname -s)" in
    MSYS_NT-*|MINGW*_NT-*) ;;
    *)
        if output=$("$BASH" "$installer" 2>&1); then
            printf 'Unsupported platform was accepted\n' >&2
            exit 1
        fi
        [[ $output == *'requires MSYS2 or Git Bash'* ]]
        ;;
esac
