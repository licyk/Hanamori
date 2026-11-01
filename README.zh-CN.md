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
    <img src="https://flat.badgen.net/github/last-commit/licyk/Hanamori/main?icon=github&color=green&label=last%20main%20commit" alt="main 分支最近提交">
  </a>
  <a href="https://github.com/licyk/Hanamori/actions/workflows/ci.yml">
    <img src="https://github.com/licyk/Hanamori/actions/workflows/ci.yml/badge.svg" alt="CI">
  </a>
  <a href="https://github.com/licyk/Hanamori/actions/workflows/release.yml">
    <img src="https://github.com/licyk/Hanamori/actions/workflows/release.yml/badge.svg" alt="Release">
  </a>
  <a href="https://github.com/licyk/Hanamori/releases">
    <img src="https://img.shields.io/github/v/release/licyk/Hanamori?style=flat&logo=github&logoColor=silver&color=bluegreen&labelColor=grey" alt="GitHub 发布版本">
  </a>
  <a href="https://crates.io/crates/hanamori">
    <img src="https://img.shields.io/crates/v/hanamori?style=flat&logo=rust&logoColor=silver&color=bluegreen&labelColor=grey" alt="crates.io 版本">
  </a>
  <a href="https://crates.io/crates/hanamori">
    <img src="https://img.shields.io/crates/d/hanamori?style=flat&logo=rust&logoColor=silver&color=bluegreen&labelColor=grey" alt="crates.io 下载量">
  </a>
  <a href="https://github.com/licyk/Hanamori/blob/main/LICENSE">
    <img src="https://img.shields.io/badge/licence-GPL--3.0--only-bluegreen?style=flat&labelColor=grey" alt="许可证：GPL-3.0-only">
  </a>
</p>

[English](README.md) · **简体中文**

</div>

Hanamori 在 Windows x64 / ARM64 上管理独立的 MSVC、Windows SDK 和 MSBuild 安装。无需安装完整的 Visual Studio，即可安装工具链、为项目固定版本、切换当前 Shell 环境，或使用指定工具链运行命令。

支持别名与默认工具链、共享下载缓存、Shell 初始化、按目录发现配置、重新安装、自我更新和按范围卸载。支持 PowerShell、Bash 和 Zsh 集成；CMD 集成仍处于实验阶段。

- **官网：** <https://hanamori.netlify.app/>
- **文档：** <https://hanamori.netlify.app/docs/>（中文与英文）

## 快速开始

在 Windows PowerShell 5.1 或 PowerShell 7 中安装 Hanamori：

```powershell
irm https://hanamori.netlify.app/install.ps1 | iex
```

脚本将 Windows x64 / ARM64 程序安装到 `%LOCALAPPDATA%\Hanamori\bin`，将该目录加入用户 PATH，并为当前 PowerShell 的 Profile 和会话初始化 Hook 与补全。无需管理员权限，工具链需要另行安装。

两个安装脚本均支持通过 `HANAMORI_INSTALL_DIR` 指定程序安装目录。显式传入的 `-InstallDir` / `--install-dir` 参数优先；环境变量未设置或为空时使用默认目录。`HANAMORI_HOME` 单独控制管理器数据目录。

在 MSYS2 或 Git Bash 中，使用 Bash 安装脚本：

```bash
curl -fsSL https://hanamori.netlify.app/install.sh | bash
```

脚本使用相同的程序目录和用户 PATH，并配置 Bash 启动文件与补全。安装后打开新的 Bash 终端，或执行脚本输出的 Hook 加载命令。需要 `cygpath`、`curl` 和 Windows PowerShell 或 PowerShell 7。随后按需安装工具链：

```powershell
hanamori install latest --name stable
hanamori default stable
hanamori exec '--' cl.exe /nologo /EHsc /MT hello.cpp
.\hello.exe
```

[首次编译指南](https://hanamori.netlify.app/docs/quick-start) 包含完整的 `hello.cpp` 示例和 Shell 激活方法。

使用 `--arch x64/arm64` 安装交叉编译工具，或 `--arch arm64/arm64` 安装原生 ARM64 工具；省略时跟随 Windows 原生架构。

默认使用 VS 2026 stable，安装 MSVC、Windows SDK 和 MSBuild。MSBuild 支持 VS 2026 v145 和 VS 2022 v143（指定 `--vs-version 2022`）原生 C++ 项目与解决方案：

```powershell
hanamori install latest --name build
hanamori exec --using build '--' MSBuild.exe app.sln -p:Configuration=Release -p:Platform=x64
```

`--no-msbuild` 仅安装 MSVC 和 SDK；`--no-sdk` 默认仅安装编译器。显式传入的 `--msbuild` / `--no-msbuild` 会覆盖项目的 MSBuild 设置。

当前初步支持 C++ 构建目标与文件跟踪，依赖 Windows SDK 和主机的 .NET Framework，并设有单独的[原生验收测试](https://hanamori.netlify.app/docs/testing)。Windows 原生构建验收仍待完成；Linux 解包测试不能替代该验收。支持范围见[工具链安装文档](https://hanamori.netlify.app/docs/installation)。

## 卸载

按名称卸载受管工具链：

```powershell
hanamori uninstall stable
```

卸载 Hanamori 本体前，如果当前 Shell 已激活工具链，先执行 `hanamori deactivate`，并关闭其他 Hanamori 会话。先预览清理范围，再执行卸载：

```powershell
hanamori self uninstall --dry-run
hanamori self uninstall
```

这会删除程序、升级备份、已记录且仍能确认归属的 Shell 启动入口，以及安装脚本添加的用户 PATH 条目。工具链、配置和命名引用会保留。

若还需要清理受管工具链、已完整下载的缓存、配置和引用：

```powershell
hanamori self uninstall --purge --dry-run
hanamori self uninstall --purge
```

受管工具链目录会整体删除。外部工具链文件、受管工具链目录之外的未知文件，以及手动添加的 PATH 条目会保留。在 Windows 上，辅助脚本会在主进程退出后完成程序删除。清理边界和失败重试方法见[卸载指南](https://hanamori.netlify.app/docs/self-uninstall)。

## 开发

```sh
cargo build --locked --release -p hanamori --bin hanamori
cargo test --workspace --locked
cargo xtask check
```

构建 Windows MSVC 程序需要 Rust 和对应的链接器、构建工具。修改或发布 CLI 前，请阅读[架构](https://hanamori.netlify.app/docs/architecture)、[测试](https://hanamori.netlify.app/docs/testing)和[发布流程](https://hanamori.netlify.app/docs/release)。

独立网站位于 `site/`：

```sh
cd site
bun install --frozen-lockfile
bun run dev
```

运行 `bun run build` 将静态文件输出到 `site/.output/public`，通过 `bun run check` 和 `bun run test` 进行验证。双语内容和部署配置见[网站维护文档](https://hanamori.netlify.app/docs/website)。网站托管需要单独配置，仅添加这些文件不会发布网站。

本项目采用 [GNU General Public License v3.0](LICENSE) 许可证。网站引用与署名见 [site/NOTICE](site/NOTICE)。
