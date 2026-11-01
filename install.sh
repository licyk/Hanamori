#!/usr/bin/env bash

main() (
    set -euo pipefail
    local version=latest architecture=auto install_dir=${HANAMORI_INSTALL_DIR:-} no_path=false no_shell=false
    while (($#)); do
        case "$1" in
            --version|--install-dir|--architecture)
                if (($# < 2)) || [[ -z $2 || $2 == --* ]]; then
                    printf 'Missing value for %s\n' "$1" >&2
                    return 1
                fi
                case "$1" in
                    --version) version=$2 ;;
                    --architecture) architecture=$2 ;;
                    --install-dir) install_dir=$2 ;;
                esac
                shift 2
                ;;
            --no-path-update) no_path=true; shift ;;
            --no-shell) no_shell=true; shift ;;
            -h|--help) usage; return ;;
            *) printf 'Unknown option: %s\n' "$1" >&2; usage >&2; return 1 ;;
        esac
    done
    case "$architecture" in auto|x64|arm64) ;; *) printf 'Architecture must be auto, x64 or arm64.\n' >&2; return 1 ;; esac
    require_msys
    if [[ -z $install_dir ]]; then
        : "${LOCALAPPDATA:?LOCALAPPDATA is required to locate the installation directory}"
        install_dir="$LOCALAPPDATA/Hanamori/bin"
    fi
    install_dir=$(cygpath -au -- "$install_dir")
    local powershell backend staging= script_dir=
    powershell=$(find_powershell)
    if [[ -f ${BASH_SOURCE[0]:-} ]]; then
        script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
    fi
    if [[ -n $script_dir && -f $script_dir/install.ps1 ]]; then
        backend="$script_dir/install.ps1"
    else
        command -v curl >/dev/null || { printf 'curl is required to download install.ps1.\n' >&2; return 1; }
        staging=$(mktemp -d)
        trap 'rm -rf -- "$staging"' EXIT
        backend="$staging/install.ps1"
        curl --fail --show-error --location --connect-timeout 30 --max-time 120 \
            https://hanamori.netlify.app/install.ps1 --output "$backend"
    fi
    local -a arguments=(-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass
        -File "$(cygpath -aw -- "$backend")" -Version "$version"
        -InstallDir "$(cygpath -aw -- "$install_dir")" -Architecture "$architecture" -NoShell)
    if $no_path; then arguments+=(-NoPathUpdate); fi
    # All native arguments are converted explicitly, including paths with spaces.
    MSYS2_ARG_CONV_EXCL='*' MSYS_NO_PATHCONV=1 "$powershell" "${arguments[@]}"

    local executable="$install_dir/hanamori.exe"
    if ! $no_shell; then
        MSYS2_ARG_CONV_EXCL='*' MSYS_NO_PATHCONV=1 "$executable" shell init \
            --shell bash --shell-executable "$(cygpath -aw -- "$BASH")" --completions
        printf '\nOpen a new Bash terminal, or load the hook in this terminal:\n'
        printf '  eval "$(%q env --shell bash --completions)"\n' "$executable"
    else
        printf '\nInstalled %s. Open a new terminal to reload the user PATH.\n' "$executable"
    fi
)

usage() {
    cat <<'USAGE'
Usage: bash install.sh [options]
Install Hanamori for Windows x64 or ARM64 from MSYS2 or Git Bash.

  --architecture ARCH     Hanamori binary: auto (native Windows), x64 or arm64
  --version VERSION       Release version or latest (default)
  --install-dir DIRECTORY Directory containing hanamori.exe (Windows or MSYS path)
  --no-path-update        Skip user PATH configuration
  --no-shell              Skip Bash profile integration and completions
  -h, --help              Show this help

HANAMORI_INSTALL_DIR sets the installation directory when --install-dir is omitted.
An unset or empty variable defaults to %LOCALAPPDATA%/Hanamori/bin.
USAGE
}

require_msys() {
    case "$(uname -s)" in
        MSYS_NT-*|MINGW*_NT-*) ;;
        *) printf 'This installer requires MSYS2 or Git Bash on Windows x64 or ARM64.\n' >&2; return 1 ;;
    esac
    command -v cygpath >/dev/null || { printf 'cygpath is required; run from MSYS2 or Git Bash.\n' >&2; return 1; }
}

find_powershell() {
    local program windows_directory
    for program in powershell.exe pwsh.exe; do
        if command -v "$program" >/dev/null; then command -v "$program"; return; fi
    done
    windows_directory=${SYSTEMROOT:-${SystemRoot:-}}
    if [[ -n $windows_directory ]]; then
        program="$(cygpath -u -- "$windows_directory")/System32/WindowsPowerShell/v1.0/powershell.exe"
        if [[ -f $program ]]; then printf '%s\n' "$program"; return; fi
    fi
    printf 'Windows PowerShell or PowerShell 7 is required.\n' >&2
    return 1
}

main "$@"
