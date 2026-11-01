set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"
root=$(cygpath -au -- "$INSTALLER_ROOT")
export HOME="$root/home"
mkdir -p -- "$HOME"
printf '# user bashrc\n' > "$HOME/.bashrc"
printf '# user login\n' > "$HOME/.bash_profile"
installer="$root/install.sh"
printf 'Bash runtime: %s\ncygpath: %s\nInstaller: %s\n' \
    "$(cygpath -aw -- "$BASH")" "$(command -v cygpath)" "$installer" >&2
directory="$root/custom bin"
export HANAMORI_INSTALL_DIR="$directory"
"$BASH" "$installer"
test -f "$directory/hanamori.exe.install.json"
grep -q '# user bashrc' "$HOME/.bashrc"
grep -q '# user login' "$HOME/.bash_profile"
grep -q '# >>> hanamori initialize >>>' "$HOME/.bashrc"
grep -q '# >>> hanamori initialize >>>' "$HOME/.bash_profile"
test "$("$BASH" --noprofile -ic 'hanamori --version' | tr -d '\r')" = "hanamori $INSTALLER_VERSION"
cp -- "$HOME/.bashrc" "$root/rc-before"
cp -- "$HOME/.bash_profile" "$root/login-before"
export HANAMORI_INSTALL_DIR="$root/unused-env-bin"
"$BASH" "$installer" --install-dir "$(cygpath -aw -- "$directory")" --version "$INSTALLER_VERSION"
test ! -e "$HANAMORI_INSTALL_DIR"
cmp -- "$HOME/.bashrc" "$root/rc-before"
cmp -- "$HOME/.bash_profile" "$root/login-before"
cat -- "$installer" | HANAMORI_INSTALL_DIR="$(cygpath -aw -- "$root/piped")" "$BASH" -s -- --no-path-update --no-shell
test -f "$root/piped/hanamori.exe"
cp -- "$directory/hanamori.exe" "$root/before.exe"
if "$BASH" "$installer" --install-dir "$directory" --version 9.9.9 > "$root/failure.log" 2>&1; then
    printf 'Corrupt release was accepted\n' >&2
    exit 1
fi
cat -- "$root/failure.log"
grep -q 'SHA-256' "$root/failure.log"
cmp -- "$directory/hanamori.exe" "$root/before.exe"
"$BASH" "$installer" --install-dir "$root/bare" --no-path-update --no-shell
test -f "$root/bare/hanamori.exe"
test ! -f "$root/bare/hanamori.exe.install.json"
HANAMORI_INSTALL_DIR='' "$BASH" "$installer" --no-path-update --no-shell
test -f "$(cygpath -au -- "$LOCALAPPDATA")/Hanamori/bin/hanamori.exe"
cmp -- "$HOME/.bashrc" "$root/rc-before"
cmp -- "$HOME/.bash_profile" "$root/login-before"
