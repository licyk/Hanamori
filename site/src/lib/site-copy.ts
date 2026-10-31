import { useI18n } from "fumadocs-ui/contexts/i18n";
import { DEFAULT_LOCALE, isLocale, type Locale } from "@/lib/i18n";
const zhCN = {
  "pageRailLabel": "页面区块导航",
  "theme": {
    "hint": "点击切换主题",
    "modes": {
      "system": "主题：跟随系统",
      "light": "主题：浅色",
      "dark": "主题：深色"
    }
  },
  "docs": {
    "comments": "评论"
  },
  "home": {
    "sections": [
      "首页",
      "功能",
      "使用流程",
      "特点",
      "下载"
    ],
    "hero": {
      "eyebrow": "独立 MSVC 工具链管理器",
      "title": "为项目准备",
      "titleAccent": "合适的工具链",
      "description": "独立安装 MSVC 与 Windows SDK，按项目选择版本。无需安装 Visual Studio，就能在终端中编译和构建。",
      "start": "下载 Hanamori",
      "learn": "了解",
      "platformsLabel": "支持的平台"
    },
    "features": {
      "title": "从安装到编译，都在终端完成",
      "description": "把编译器、SDK、项目选择和环境管理放在同一套命令中。",
      "open": "查看文档",
      "items": [
        {
          "title": "独立安装",
          "description": "每套 MSVC/SDK 组合拥有独立目录，按需选择版本或仅安装编译器。",
          "href": "/docs/installation"
        },
        {
          "title": "项目选择",
          "description": "用项目文件声明版本，通过托管目录键、版本前缀或别名选择工具链。",
          "href": "/docs/projects"
        },
        {
          "title": "Shell 集成",
          "description": "在 PowerShell、Bash、Zsh 中切换与恢复环境，提供实验性 CMD 支持。",
          "href": "/docs/shell"
        },
        {
          "title": "单次执行",
          "description": "使用 exec 为一个子进程准备编译环境，不必先初始化终端。",
          "href": "/docs/execution"
        },
        {
          "title": "导入、重装与清理",
          "description": "引用外部目录，检查实际文件，重新安装缺失工具，并清理托管目录。",
          "href": "/docs/maintenance"
        }
      ]
    },
    "workflow": {
      "title": "从下载到第一个 C++ 程序",
      "description": "四个步骤，建立可重复使用的编译环境。",
      "steps": [
        {
          "title": "获取程序",
          "description": "下载 Windows x64 版本，或按文档从源码构建。"
        },
        {
          "title": "安装工具链",
          "description": "使用 install 选择 MSVC 与 SDK，再为组合设置一个别名。"
        },
        {
          "title": "选择环境",
          "description": "设置项目文件，在当前终端 use，或者直接 exec。"
        },
        {
          "title": "编译与运行",
          "description": "编译 hello.cpp，运行程序，并检查退出状态。"
        }
      ]
    },
    "highlights": {
      "title": "管理边界清晰，项目彼此独立",
      "description": "工具链选择显式可见，环境变更可以恢复。",
      "items": [
        {
          "title": "不依赖 Visual Studio 安装",
          "description": "直接解析微软清单与安装包，不依赖 aria2c 或 msiexec。"
        },
        {
          "title": "独立的安装目录",
          "description": "托管组合独立保存；导入仅建立只读引用，不取得目录删除权。"
        },
        {
          "title": "可恢复的会话",
          "description": "切换版本不会累积 PATH，退出时恢复激活前的环境。"
        },
        {
          "title": "共享完整下载缓存",
          "description": "工具链目录保持独立，完整 payload 可以通过内容寻址缓存复用。"
        },
        {
          "title": "先查看，再清理",
          "description": "卸载和清理提供预览；错误指出保留的内容和恢复方式。"
        }
      ]
    },
    "install": {
      "title": "准备好开始了吗？",
      "description": "获取程序，查看完整的首次编译教程，或按命令参考配置你的工作环境。",
      "link": "前往下载页"
    },
    "footer": {
      "label": "页脚导航",
      "docs": "文档",
      "quickStart": "快速开始",
      "faq": "故障排查",
      "repository": "源代码",
      "socialLabel": "社交链接",
      "social": {
        "blog": "博客",
        "github": "GitHub",
        "bilibili": "哔哩哔哩"
      }
    }
  }
} as const;
type TranslationShape<T> = T extends string ? string : T extends readonly unknown[] ? { readonly [K in keyof T]: TranslationShape<T[K]> } : T extends object ? { readonly [K in keyof T]: TranslationShape<T[K]> } : T;
export type SiteCopy = TranslationShape<typeof zhCN>;
const en = {
  "pageRailLabel": "Page sections",
  "theme": {
    "hint": "Click to switch theme",
    "modes": {
      "system": "Theme: follow system",
      "light": "Theme: light",
      "dark": "Theme: dark"
    }
  },
  "docs": {
    "comments": "Comments"
  },
  "home": {
    "sections": [
      "Home",
      "Features",
      "Workflow",
      "Highlights",
      "Download"
    ],
    "hero": {
      "eyebrow": "Standalone MSVC toolchain manager",
      "title": "The right build tools,",
      "titleAccent": "for every project",
      "description": "Install MSVC and the Windows SDK in independent directories. Select a toolchain for each project and build from your terminal without a Visual Studio installation.",
      "start": "Download Hanamori",
      "learn": "Explore ",
      "platformsLabel": "Supported platform"
    },
    "features": {
      "title": "From installation to compilation, in your terminal",
      "description": "Manage compilers, SDKs, project selection and environments with one set of commands.",
      "open": "Read the guide",
      "items": [
        {
          "title": "Independent installations",
          "description": "Give each MSVC/SDK combination its own directory, or install just the compiler.",
          "href": "/docs/installation"
        },
        {
          "title": "Project selection",
          "description": "Declare versions in a project file and select by managed directory key, version prefix or alias.",
          "href": "/docs/projects"
        },
        {
          "title": "Shell integration",
          "description": "Switch and restore environments in PowerShell, Bash and Zsh, with experimental CMD support.",
          "href": "/docs/shell"
        },
        {
          "title": "Run one command",
          "description": "Prepare an environment for a child process with exec, without initializing your shell.",
          "href": "/docs/execution"
        },
        {
          "title": "Import, reinstall and remove",
          "description": "Reference external directories, inspect installed tools, reinstall missing files and remove managed toolchains.",
          "href": "/docs/maintenance"
        }
      ]
    },
    "workflow": {
      "title": "From download to your first C++ program",
      "description": "Four steps to a reusable build environment.",
      "steps": [
        {
          "title": "Get Hanamori",
          "description": "Download the Windows x64 build or compile it from source."
        },
        {
          "title": "Install a toolchain",
          "description": "Choose MSVC and the SDK with install, then name the combination with an alias."
        },
        {
          "title": "Select an environment",
          "description": "Use a project file, activate this terminal with use, or invoke exec directly."
        },
        {
          "title": "Build and run",
          "description": "Compile hello.cpp, run the program and check the exit status."
        }
      ]
    },
    "highlights": {
      "title": "Clear ownership, independent projects",
      "description": "Toolchain selection is explicit and environment changes are reversible.",
      "items": [
        {
          "title": "No Visual Studio installation required",
          "description": "Reads Microsoft manifests and packages directly, without aria2c or msiexec."
        },
        {
          "title": "Separate installation directories",
          "description": "Owned combinations are independent. Imports are read-only references, without directory deletion rights."
        },
        {
          "title": "Restorable sessions",
          "description": "Switching versions does not accumulate PATH entries. Deactivation restores the original environment."
        },
        {
          "title": "Shared complete download cache",
          "description": "Installations remain separate while complete payloads can be reused from a content-addressed cache."
        },
        {
          "title": "Inspect before removing",
          "description": "Preview uninstall and cleanup operations. Errors identify retained data and recovery steps."
        }
      ]
    },
    "install": {
      "title": "Ready to build?",
      "description": "Get Hanamori, follow the first compilation tutorial, or configure your environment with the command reference.",
      "link": "Go to downloads"
    },
    "footer": {
      "label": "Footer navigation",
      "docs": "Docs",
      "quickStart": "Quick start",
      "faq": "Troubleshooting",
      "repository": "Source code",
      "socialLabel": "Social links",
      "social": {
        "blog": "Blog",
        "github": "GitHub",
        "bilibili": "Bilibili"
      }
    }
  }
} as const satisfies SiteCopy;
const SITE_COPY: Record<Locale, SiteCopy> = { "zh-CN": zhCN, en };
export function getSiteCopy(locale: Locale): SiteCopy { return SITE_COPY[locale]; }
export function useSiteLocale(): Locale { const {locale}=useI18n(); return isLocale(locale) ? locale : DEFAULT_LOCALE; }
export function useSiteCopy(): SiteCopy { return getSiteCopy(useSiteLocale()); }
