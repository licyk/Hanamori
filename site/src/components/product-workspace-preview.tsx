import { Pause, Play, Terminal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useSiteLocale } from "@/lib/site-copy";
const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";
const commands = [
  "hanamori install latest --name stable",
  "hanamori default stable",
  "hanamori exec --using stable '--' cl.exe /nologo /EHsc /MT hello.cpp",
  ".\\hello.exe"
] as const;
const labels = {
  "zh-CN": [
    "安装",
    "选择",
    "编译",
    "运行"
  ],
  "en": [
    "Install",
    "Select",
    "Compile",
    "Run"
  ]
};
const outputs = {
  "zh-CN": [
    "MSVC + Windows SDK → 独立安装目录\nstable → 托管目录键",
    "default → stable 对应的托管目录键\n当前终端环境保持不变",
    "hello.cpp\nhello.exe",
    "Hello from Hanamori!\n退出状态：0"
  ],
  "en": [
    "MSVC + Windows SDK → independent directory\nstable → managed directory key",
    "default → the directory key selected by stable\nThe current shell environment is unchanged",
    "hello.cpp\nhello.exe",
    "Hello from Hanamori!\nExit status: 0"
  ]
};
export default function ProductWorkspacePreview() {
  const locale = useSiteLocale();
  const rootRef = useRef<HTMLElement>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const [isAutoPlaying, setIsAutoPlaying] = useState(true);
  const [isInteractionPaused, setIsInteractionPaused] = useState(false);
  const [isPageVisible, setIsPageVisible] = useState(true);
  const [isOnScreen, setIsOnScreen] = useState(false);
  const [prefersReducedMotion, setPrefersReducedMotion] = useState(false);

  useEffect(() => {
    const media = window.matchMedia(REDUCED_MOTION_QUERY);

    function syncMotionPreference() {
      setPrefersReducedMotion(media.matches);
    }

    syncMotionPreference();
    media.addEventListener("change", syncMotionPreference);
    return () => media.removeEventListener("change", syncMotionPreference);
  }, []);

  useEffect(() => {
    function syncPageVisibility() {
      setIsPageVisible(document.visibilityState === "visible");
    }

    syncPageVisibility();
    document.addEventListener("visibilitychange", syncPageVisibility);
    return () => document.removeEventListener("visibilitychange", syncPageVisibility);
  }, []);

  // Scrolled past, the preview has nothing to show; stop advancing rather than
  // re-rendering and running the progress animation for no one.
  useEffect(() => {
    const root = rootRef.current;
    if (!root || typeof window.IntersectionObserver !== "function") {
      setIsOnScreen(true);
      return;
    }

    const observer = new IntersectionObserver(([entry]) => setIsOnScreen(entry.isIntersecting), {
      threshold: 0.2,
    });

    observer.observe(root);
    return () => observer.disconnect();
  }, []);

  const cycling = isAutoPlaying && isOnScreen && isPageVisible && !prefersReducedMotion && !isInteractionPaused;
  const playbackLabel = locale === "zh-CN"
    ? (isAutoPlaying ? "暂停演示" : "播放演示")
    : (isAutoPlaying ? "Pause demo" : "Play demo");

  return (
    <figure
      className="product-preview page-interactive"
      ref={rootRef}
      aria-label={locale === "zh-CN" ? "Hanamori 命令操作示意" : "Illustrative Hanamori command sequence"}
      onMouseEnter={() => setIsInteractionPaused(true)}
      onMouseLeave={() => setIsInteractionPaused(false)}
      onFocusCapture={() => setIsInteractionPaused(true)}
      onBlurCapture={event => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setIsInteractionPaused(false);
      }}
    >
      <div className="product-preview-window page-interactive-part">
        <div className="terminal-title">
          <Terminal size={18} aria-hidden="true" />
          <span>Hanamori · PowerShell</span>
          <button type="button" onClick={() => setIsAutoPlaying(!isAutoPlaying)} aria-label={playbackLabel}>
            {isAutoPlaying ? <Pause size={16} /> : <Play size={16} />}
          </button>
        </div>
        <div className="terminal-steps" aria-label={locale === "zh-CN" ? "演示步骤" : "Demo steps"}>
          {labels[locale].map((label, index) => (
            <button
              type="button"
              key={label}
              aria-pressed={activeIndex === index}
              onClick={() => { setActiveIndex(index); setIsAutoPlaying(false); }}
            >
              {index + 1}. {label}
            </button>
          ))}
        </div>
        <div className="terminal-content">
          <p className="terminal-prompt">PS C:\projects\hello&gt;</p>
          <pre><code>{commands[activeIndex]}</code></pre>
          <pre className="terminal-output">{outputs[locale][activeIndex]}</pre>
        </div>
        <figcaption className="terminal-caption">
          {locale === "zh-CN" ? "操作示意 · 版本与输出以实际安装为准" : "Illustration · versions and output depend on your installation"}
        </figcaption>
        <div
          className="terminal-progress"
          key={activeIndex}
          style={{ animationPlayState: cycling ? "running" : "paused" }}
          onAnimationEnd={() => setActiveIndex(index => (index + 1) % commands.length)}
        />
      </div>
    </figure>
  );
}
