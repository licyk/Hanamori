<div align="center">

# Hanamori

<p align="center">
  <a href="https://github.com/licyk/Hanamori/stargazers">
    <img src="https://img.shields.io/github/stars/licyk/Hanamori?style=flat&logo=github&logoColor=silver&color=bluegreen&labelColor=grey" alt="Stars">
  </a>
  <a href="https://github.com/licyk/Hanamori/issues">
    <img src="https://img.shields.io/github/issues/licyk/Hanamori?style=flat&logo=github&logoColor=silver&color=bluegreen&labelColor=grey" alt="Issues">
  </a>
  <a href="https://github.com/licyk/Hanamori/commits/main">
    <img src="https://flat.badgen.net/github/last-commit/licyk/Hanamori/main?icon=github&color=green&label=last%20main%20commit" alt="Last main commit">
  </a>
  <a href="https://github.com/licyk/Hanamori/actions/workflows/ci.yml">
    <img src="https://github.com/licyk/Hanamori/actions/workflows/ci.yml/badge.svg" alt="CI">
  </a>
  <a href="https://github.com/licyk/Hanamori/actions/workflows/release.yml">
    <img src="https://github.com/licyk/Hanamori/actions/workflows/release.yml/badge.svg" alt="Release">
  </a>
  <a href="https://github.com/licyk/Hanamori/releases">
    <img src="https://img.shields.io/github/v/release/licyk/Hanamori?style=flat&logo=github&logoColor=silver&color=bluegreen&labelColor=grey" alt="GitHub release">
  </a>
  <a href="https://crates.io/crates/hanamori">
    <img src="https://img.shields.io/crates/v/hanamori?style=flat&logo=rust&logoColor=silver&color=bluegreen&labelColor=grey" alt="crates.io version">
  </a>
  <a href="https://crates.io/crates/hanamori">
    <img src="https://img.shields.io/crates/d/hanamori?style=flat&logo=rust&logoColor=silver&color=bluegreen&labelColor=grey" alt="crates.io downloads">
  </a>
  <a href="https://github.com/licyk/Hanamori/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/licence-GPL--3.0--only-bluegreen?style=flat&labelColor=grey" alt="Licence: GPL-3.0-only">
  </a>
</p>

**English** · [简体中文](README.zh-CN.md)

</div>

Hanamori manages independent MSVC, Windows SDK and MSBuild installations on Windows x64. Install toolchains without a full Visual Studio installation, pin projects to specific versions, switch the current shell environment, or run a command with a selected toolchain.

It provides aliases and defaults, shared download caching, shell initialization, directory-based discovery and reinstallation, self-update, and scoped uninstallation. PowerShell, Bash and Zsh integration are supported; CMD integration is experimental.

This repository describes the current development version. Historical `msvc-installer` release assets do not contain the current commands. The website download page offers only actual matching `hanamori` assets; a private repository does not provide anonymous downloads.

- **Website:** <https://licyk.github.io/Hanamori/>
- **Documentation:** <https://licyk.github.io/Hanamori/docs/> (English and Chinese)

## Get started

Install Hanamori in Windows PowerShell 5.1 or PowerShell 7:

```powershell
irm https://licyk.github.io/Hanamori/install.ps1 | iex
```

The script installs the Windows x64 executable into `%LOCALAPPDATA%\Hanamori\bin`, adds that directory to your user PATH, and initializes the current PowerShell profile and session with completions. No administrator privileges are required. Toolchains are installed separately.

Set `HANAMORI_INSTALL_DIR` to choose the executable directory in either installer. Explicit `-InstallDir` / `--install-dir` arguments take precedence; an unset or empty variable uses the default directory. `HANAMORI_HOME` separately selects manager data.

For MSYS2 or Git Bash, use the Bash installer:

```bash
curl -fsSL https://licyk.github.io/Hanamori/install.sh | bash
```

It uses the same executable directory and user PATH, and configures Bash startup files and completions. Open a new Bash terminal afterward, or run the printed hook command. It requires `cygpath`, `curl` and Windows PowerShell or PowerShell 7. Install a toolchain when needed:

```powershell
hanamori install latest --name stable
hanamori default stable
hanamori exec '--' cl.exe /nologo /EHsc /MT hello.cpp
.\hello.exe
```

The [first compilation guide](site/content/docs/quick-start.en.mdx) includes a complete `hello.cpp` and shell activation instructions.

Installation includes MSVC, the Windows SDK and MSBuild by default. MSBuild supports VS 2022 x64/v143 native C++ projects and solutions:

```powershell
hanamori install latest --name build
hanamori exec --using build '--' MSBuild.exe app.sln -p:Configuration=Release -p:Platform=x64
```

`--no-msbuild` installs MSVC and the SDK; `--no-sdk` defaults to a compiler-only installation. Explicit `--msbuild` / `--no-msbuild` override a project’s MSBuild setting.

This initial support includes C++ targets and file tracking, requires a Windows SDK and the host .NET Framework, and has a separate [native acceptance gate](site/content/docs/testing.en.mdx). Native Windows build acceptance remains pending; Linux extraction tests do not establish it. See [installation](site/content/docs/installation.en.mdx) for the supported scope.

## Uninstall

Remove a managed toolchain by name:

```powershell
hanamori uninstall stable
```

To remove Hanamori itself, run `hanamori deactivate` first if this shell has an active toolchain, and close other Hanamori sessions. Preview the removal, then uninstall:

```powershell
hanamori self uninstall --dry-run
hanamori self uninstall
```

This removes the executable, its upgrade backup, recorded shell startup entries that still belong to Hanamori, and the user PATH entry added by the installer. Toolchains, configuration and named references are preserved.

To also remove managed toolchains, complete cached downloads, configuration and references:

```powershell
hanamori self uninstall --purge --dry-run
hanamori self uninstall --purge
```

Managed toolchain directories are removed in full. External toolchain files, unknown files outside those directories, and manually added PATH entries are preserved. On Windows, executable removal finishes through a helper after the main process exits. See the [uninstall guide](site/content/docs/self-uninstall.en.mdx) for cleanup boundaries and retry instructions.

## Development

```sh
cargo build --locked --release -p hanamori --bin hanamori
cargo test --workspace --locked
cargo xtask check
```

Building the Windows MSVC executable requires Rust and the corresponding linker/build tools. Read [architecture](site/content/docs/architecture.en.mdx), [testing](site/content/docs/testing.en.mdx), and [release](site/content/docs/release.en.mdx) before changing or publishing the CLI.

The independent website lives in `site/`:

```sh
cd site
bun install --frozen-lockfile
bun run dev
```

Use `bun run build` for static output in `site/.output/public`, then `bun run check` and `bun run test` for validation. See [website maintenance](site/content/docs/website.en.mdx) for bilingual content and deployment configuration. Hosting must be configured separately; adding these files does not publish a website.

Licensed under the [GNU General Public License v3.0](LICENSE). Website attribution is in [site/NOTICE](site/NOTICE).
