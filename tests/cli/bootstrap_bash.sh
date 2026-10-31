set -euo pipefail
root=$(cygpath -au -- "$INSTALLER_ROOT")
export HOME="$root/home"
export PATH="/usr/bin:/bin:$PATH"
mkdir -p -- "$HOME"
printf '# user bashrc\n' > "$HOME/.bashrc"
printf '# user login\n' > "$HOME/.bash_profile"
installer="$root/install.sh"
directory="$root/custom bin"
"$BASH" "$installer" --install-dir "$directory"
test -f "$directory/hanamori.exe.install.json"
grep -q '# user bashrc' "$HOME/.bashrc"
grep -q '# user login' "$HOME/.bash_profile"
grep -q '# >>> hanamori initialize >>>' "$HOME/.bashrc"
grep -q '# >>> hanamori initialize >>>' "$HOME/.bash_profile"
test "$("$BASH" --noprofile -ic 'hanamori --version' | tr -d '\r')" = "hanamori $INSTALLER_VERSION"
cp -- "$HOME/.bashrc" "$root/rc-before"
cp -- "$HOME/.bash_profile" "$root/login-before"
"$BASH" "$installer" --install-dir "$(cygpath -aw -- "$directory")" --version "$INSTALLER_VERSION"
cmp -- "$HOME/.bashrc" "$root/rc-before"
cmp -- "$HOME/.bash_profile" "$root/login-before"
cat -- "$installer" | "$BASH" -s -- --install-dir "$root/piped" --no-path-update --no-shell
test -f "$root/piped/hanamori.exe"
cp -- "$directory/hanamori.exe" "$root/before.exe"
if "$BASH" "$installer" --install-dir "$directory" --version 9.9.9; then
    printf 'Corrupt release was accepted\n' >&2
    exit 1
fi
cmp -- "$directory/hanamori.exe" "$root/before.exe"
"$BASH" "$installer" --install-dir "$root/bare" --no-path-update --no-shell
test -f "$root/bare/hanamori.exe"
test ! -f "$root/bare/hanamori.exe.install.json"
cmp -- "$HOME/.bashrc" "$root/rc-before"
cmp -- "$HOME/.bash_profile" "$root/login-before"
