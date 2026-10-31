# Hanamori

Hanamori manages independent MSVC, Windows SDK and MSBuild installations on Windows x64. Install toolchains without a full Visual Studio installation, pin projects to specific versions, switch the current shell environment, or run a command with a selected toolchain.

It provides aliases and defaults, shared download caching, shell initialization, directory-based discovery and reinstallation, self-update, and scoped uninstallation. PowerShell, Bash and Zsh integration are supported; CMD integration is experimental.

This repository describes the current development version. Historical `msvc-installer` release assets do not contain the current commands. The website download page offers only actual matching `hanamori` assets; a private repository does not provide anonymous downloads.

## Get started

[Releases](https://github.com/licyk/Hanamori/releases) · [Installation](site/content/docs/build.en.mdx) · [First compilation](site/content/docs/quick-start.en.mdx) · [中文文档](site/content/docs/index.mdx) · [English documentation](site/content/docs/index.en.mdx)

After placing `hanamori.exe` in a directory on PATH, use PowerShell:

```powershell
hanamori install latest --name stable
hanamori default stable
hanamori exec '--' cl.exe /nologo /EHsc /MT hello.cpp
.\hello.exe
```

The [first compilation guide](site/content/docs/quick-start.en.mdx) includes a complete `hello.cpp` and shell activation instructions. See the [CLI reference](site/content/docs/cli.en.mdx), [configuration](site/content/docs/configuration.en.mdx), [maintenance](site/content/docs/maintenance.en.mdx), and [troubleshooting](site/content/docs/troubleshooting.en.mdx) for the full behavior and deletion boundaries.

Installation includes MSVC, the Windows SDK and MSBuild by default. MSBuild supports VS 2022 x64/v143 native C++ projects and solutions:

```powershell
hanamori install latest --name build
hanamori exec --using build '--' MSBuild.exe app.sln -p:Configuration=Release -p:Platform=x64
```

`--no-msbuild` installs MSVC and the SDK; `--no-sdk` defaults to a compiler-only installation. Explicit `--msbuild` / `--no-msbuild` override a project’s MSBuild setting.

This initial support includes C++ targets and file tracking, requires a Windows SDK and the host .NET Framework, and has a separate [native acceptance gate](site/content/docs/testing.en.mdx). Native Windows build acceptance remains pending; Linux extraction tests do not establish it. See [installation](site/content/docs/installation.en.mdx) for the supported scope.

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
