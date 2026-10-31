import { createFileRoute } from "@tanstack/react-router";
import { HomeLayout } from "fumadocs-ui/layouts/home";
import Link from "fumadocs-core/link";
import { Download, ExternalLink } from "lucide-react";
import { useEffect, useState } from "react";
import { baseOptions } from "@/lib/layout.shared";
import { useSiteLocale } from "@/lib/site-copy";
import { useDocumentMetadata } from "@/lib/document-metadata";
import { fetchRelease, RELEASES_URL, type ReleaseResult } from "@/lib/releases";

const copy = {
  "zh-CN": {
    "title": "下载 Hanamori",
    "description": "获取 Windows x64 程序，然后安装你的第一套 MSVC 与 Windows SDK。",
    "loading": "正在查询公开 Release…",
    "unavailable": "暂未找到可公开下载的新版程序。仓库可能尚未公开，或最新 Release 还没有 Hanamori 资产。",
    "error": "暂时无法查询 Release。请重试，或直接打开 GitHub Release 页面查看。",
    "retry": "重新查询",
    "release": "所有 Release",
    "build": "从源码构建",
    "start": "首次编译教程",
    "platform": "Windows x64 · MSVC 构建",
    "install": "PowerShell 一键安装",
    "installDescription": "在 PowerShell 5.1 或 7 中运行。自动配置用户 PATH、当前 PowerShell 集成与补全，工具链按需安装。",
    "installGuide": "安装选项与说明",
    "bashInstall": "MSYS2 / Git Bash",
    "bashDescription": "配置 Bash 启动入口与补全。安装后重开终端，或执行脚本输出的加载命令。",
    "portable": "便携程序，无需安装 Visual Studio。EXE 可直接运行；ZIP 内为 hanamori.exe。将程序放入自己的工具目录，并按文档配置 PATH。",
    "note": "本文档面向当前开发版本。旧 msvc-installer 资产不包含这里介绍的新命令，不作为新版下载提供。",
    "update": "已安装？运行 hanamori upgrade 检查并更新。",
    "source": "源码构建和 GitHub 下载需要拥有仓库访问权限；私有资产无法供匿名用户下载。",
    "date": "发布日期",
    "exe": "下载 EXE",
    "zip": "下载 ZIP"
  },
  "en": {
    "title": "Download Hanamori",
    "description": "Get the Windows x64 executable, then install your first MSVC and Windows SDK combination.",
    "loading": "Checking public releases…",
    "unavailable": "No publicly accessible new build was found. The repository may not be public yet, or the latest release may not include Hanamori assets.",
    "error": "Could not query releases. Try again, or open the GitHub releases page directly.",
    "retry": "Try again",
    "release": "All releases",
    "build": "Build from source",
    "start": "First compilation",
    "platform": "Windows x64 · MSVC build",
    "install": "Install with PowerShell",
    "installDescription": "Run in PowerShell 5.1 or 7. Configures your user PATH, current PowerShell integration and completions. Install toolchains when needed.",
    "installGuide": "Installation options and guide",
    "bashInstall": "MSYS2 / Git Bash",
    "bashDescription": "Configures Bash startup and completions. Open a new terminal afterward, or run the printed hook command.",
    "portable": "A portable executable, without a Visual Studio installation. Run the EXE directly, or extract hanamori.exe from the ZIP. Place it in your tools directory and configure PATH using the guide.",
    "note": "These docs describe the current development version. Old msvc-installer assets do not contain the new commands and are not offered as new downloads.",
    "update": "Already installed? Run hanamori upgrade to check and update.",
    "source": "Source and GitHub downloads require repository access. Private assets are not available to anonymous visitors.",
    "date": "Published",
    "exe": "Download EXE",
    "zip": "Download ZIP"
  }
};

export const Route = createFileRoute("/download")({
  head: () => ({
    meta: [
      { title: "下载 Hanamori — Windows x64" },
      { name: "description", content: copy["zh-CN"].description },
    ],
  }),
  component: DownloadPage,
});

function DownloadPage() {
  const locale = useSiteLocale();
  const text = copy[locale];
  const [result, setResult] = useState<ReleaseResult | { status: "loading" | "error" }>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  useDocumentMetadata({ title: text.title, description: text.description });

  useEffect(() => {
    const controller = new AbortController();
    setResult({ status: "loading" });
    const timeout = window.setTimeout(() => controller.abort(), 12000);
    let active = true;
    fetchRelease(controller.signal)
      .then(value => { if (active) setResult(value); })
      .catch(() => { if (active) setResult({ status: "error" }); })
      .finally(() => window.clearTimeout(timeout));
    return () => {
      active = false;
      window.clearTimeout(timeout);
      controller.abort();
    };
  }, [attempt]);

  return (
    <HomeLayout {...baseOptions(locale)}>
      <main className="download-page">
        <span className="download-eyebrow">HANAMORI</span>
        <h1>{text.title}</h1>
        <p className="download-lead">{text.description}</p>
        <section className="download-card" aria-label={text.install}>
          <h2>{text.install}</h2>
          <p>{text.installDescription}</p>
          <pre className="download-command"><code>irm https://hanamori.netlify.app/install.ps1 | iex</code></pre>
          <h2>{text.bashInstall}</h2>
          <p>{text.bashDescription}</p>
          <pre className="download-command"><code>curl -fsSL https://hanamori.netlify.app/install.sh | bash</code></pre>
          <p><Link href="/docs/build">{text.installGuide} →</Link></p>
        </section>
        <section className="download-card" aria-label={text.platform}>
          <Download aria-hidden="true" />
          <h2>{text.platform}</h2>
          <p>{text.portable}</p>
          <div aria-live="polite">
            {result.status === "available" ? (
              <>
                <p className="download-version">
                  <a href={result.release.url}>{result.release.version}</a>
                  {" · "}{text.date} {result.release.published.slice(0, 10)}
                </p>
                <div className="download-actions">
                  {result.release.assets.map(asset => (
                    <a className="primary-action" key={asset.name} href={asset.url}>
                      <Download size={18} aria-hidden="true" />
                      {asset.name.endsWith(".zip") ? text.zip : text.exe}
                      {" · "}{(asset.size / 1024 / 1024).toFixed(1)} MiB
                    </a>
                  ))}
                </div>
              </>
            ) : (
              <p className="download-status">
                {result.status === "loading" ? text.loading : result.status === "error" ? text.error : text.unavailable}
              </p>
            )}
          </div>
          {result.status !== "loading" && result.status !== "available" ? (
            <button className="secondary-action" onClick={() => setAttempt(value => value + 1)}>
              {text.retry}
            </button>
          ) : null}
          <p className="download-note">{text.note}</p>
        </section>
        <div className="download-actions">
          <a className="secondary-action" href={RELEASES_URL}>
            {text.release}<ExternalLink size={16} aria-hidden="true" />
          </a>
          <Link className="secondary-action" href="/docs/build">{text.build}</Link>
          <Link className="secondary-action" href="/docs/quick-start">{text.start}</Link>
        </div>
        <p>
          {text.update}{" "}
          <Link href="/docs/upgrade">{locale === "zh-CN" ? "更新说明" : "Update guide"} →</Link>
        </p>
        <p className="download-note">{text.source}</p>
      </main>
    </HomeLayout>
  );
}
