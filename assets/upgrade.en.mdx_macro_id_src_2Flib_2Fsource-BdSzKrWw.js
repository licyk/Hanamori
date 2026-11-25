import{r as e}from"./rolldown-runtime-XKyaHtl9.js";import{t}from"./jsx-runtime-LKYXHABd.js";var n=e(t()),r={title:`Self-update`,description:`Update a Windows x64 / ARM64 MSVC build from a verified GitHub Release.`,icon:`CircleArrowUp`},i={contents:[{heading:void 0,content:`Windows x64 / ARM64 MSVC builds can update the executable that is currently running from the
latest stable GitHub Release:`},{heading:void 0,content:"`--check` reports the current version, latest version and executable path without\ndownloading an executable or writing files. An equal version normally needs no update;\n`--force` allows reinstalling it. Neither mode downgrades a newer build, selects a\nprerelease, or bypasses verification. `--check` and `--force` cannot be combined."},{heading:void 0,content:`The updater downloads the EXE matching the running executable architecture directly. The Release asset
must supply a SHA-256 digest; its hash and size must match, and the candidate must
successfully report the expected \`hanamori --version\` within ten seconds. Missing or
invalid metadata, download errors and failed verification leave the current executable
in place. The next invocation after a successful update uses the new version.`},{heading:void 0,content:"The existing `--proxy`, `HANAMORI_PROXY` and configuration `proxy` setting apply to both\nthe API query and download. Optional `GITHUB_TOKEN` authenticates only the API request;\nit is not sent to the asset download. Toolchain mirrors, caches, download directories\nand project files do not control self-update. Other platforms/build targets currently\nhave no self-update asset."},{heading:void 0,content:`Updating requires write access to the executable's directory. Staging is created beside
that executable, and a sibling \`<executable>.update.lock\` prevents simultaneous updates.
The lock file may remain after completion; do not delete it to bypass an active update.
The old executable is renamed to \`<executable>.outdated\`, retained while its process
exits, and removed by the next actual update. After verifying the new executable, if backup
deletion is blocked by file usage, Windows Restart Manager closes its users and retries.
Resolve any remaining error manually. These sibling names are reserved for the updater.`},{heading:void 0,content:`Replacement failure attempts to restore the original executable. The two renames are
not one atomic operation: if the process is terminated between them and the original
path is missing, rename \`<executable>.outdated\` back to the original name before retrying.
A reported rollback failure also gives the exact restore paths. Self-update does not
change registered toolchains or shell configuration.`},{heading:void 0,content:"Self-update preserves the running executable architecture: x64 builds select the x64 asset; ARM64 builds select `hanamori-aarch64-pc-windows-msvc.exe`, without switching according to OS architecture."}],headings:[]},a=[];function o(e){let t={a:`a`,code:`code`,p:`p`,pre:`pre`,span:`span`,...e.components};return(0,n.jsxs)(n.Fragment,{children:[(0,n.jsxs)(t.p,{children:[`Windows x64 / ARM64 MSVC builds can update the executable that is currently running from the
latest stable `,(0,n.jsx)(t.a,{href:`https://github.com/licyk/Hanamori/releases`,children:`GitHub Release`}),`:`]}),`
`,(0,n.jsx)(n.Fragment,{children:(0,n.jsx)(t.pre,{className:`shiki shiki-themes github-light github-dark`,style:{"--shiki-light":`#24292e`,"--shiki-dark":`#e1e4e8`,"--shiki-light-bg":`#fff`,"--shiki-dark-bg":`#24292e`},tabIndex:`0`,icon:`<svg viewBox="0 0 24 24"><path d="M 6,1 C 4.354992,1 3,2.354992 3,4 v 16 c 0,1.645008 1.354992,3 3,3 h 12 c 1.645008,0 3,-1.354992 3,-3 V 8 7 A 1.0001,1.0001 0 0 0 20.707031,6.2929687 l -5,-5 A 1.0001,1.0001 0 0 0 15,1 h -1 z m 0,2 h 7 v 3 c 0,1.645008 1.354992,3 3,3 h 3 v 11 c 0,0.564129 -0.435871,1 -1,1 H 6 C 5.4358712,21 5,20.564129 5,20 V 4 C 5,3.4358712 5.4358712,3 6,3 Z M 15,3.4140625 18.585937,7 H 16 C 15.435871,7 15,6.5641288 15,6 Z" fill="currentColor" /></svg>`,children:(0,n.jsxs)(t.code,{children:[(0,n.jsxs)(t.span,{className:`line`,children:[(0,n.jsx)(t.span,{style:{"--shiki-light":`#24292E`,"--shiki-dark":`#E1E4E8`},children:`hanamori upgrade `}),(0,n.jsx)(t.span,{style:{"--shiki-light":`#D73A49`,"--shiki-dark":`#F97583`},children:`--`}),(0,n.jsx)(t.span,{style:{"--shiki-light":`#24292E`,"--shiki-dark":`#E1E4E8`},children:`check`})]}),`
`,(0,n.jsx)(t.span,{className:`line`,children:(0,n.jsx)(t.span,{style:{"--shiki-light":`#24292E`,"--shiki-dark":`#E1E4E8`},children:`hanamori upgrade`})}),`
`,(0,n.jsxs)(t.span,{className:`line`,children:[(0,n.jsx)(t.span,{style:{"--shiki-light":`#24292E`,"--shiki-dark":`#E1E4E8`},children:`hanamori upgrade `}),(0,n.jsx)(t.span,{style:{"--shiki-light":`#D73A49`,"--shiki-dark":`#F97583`},children:`--`}),(0,n.jsx)(t.span,{style:{"--shiki-light":`#24292E`,"--shiki-dark":`#E1E4E8`},children:`force`})]})]})})}),`
`,(0,n.jsxs)(t.p,{children:[(0,n.jsx)(t.code,{children:`--check`}),` reports the current version, latest version and executable path without
downloading an executable or writing files. An equal version normally needs no update;
`,(0,n.jsx)(t.code,{children:`--force`}),` allows reinstalling it. Neither mode downgrades a newer build, selects a
prerelease, or bypasses verification. `,(0,n.jsx)(t.code,{children:`--check`}),` and `,(0,n.jsx)(t.code,{children:`--force`}),` cannot be combined.`]}),`
`,(0,n.jsxs)(t.p,{children:[`The updater downloads the EXE matching the running executable architecture directly. The Release asset
must supply a SHA-256 digest; its hash and size must match, and the candidate must
successfully report the expected `,(0,n.jsx)(t.code,{children:`hanamori --version`}),` within ten seconds. Missing or
invalid metadata, download errors and failed verification leave the current executable
in place. The next invocation after a successful update uses the new version.`]}),`
`,(0,n.jsxs)(t.p,{children:[`The existing `,(0,n.jsx)(t.code,{children:`--proxy`}),`, `,(0,n.jsx)(t.code,{children:`HANAMORI_PROXY`}),` and configuration `,(0,n.jsx)(t.code,{children:`proxy`}),` setting apply to both
the API query and download. Optional `,(0,n.jsx)(t.code,{children:`GITHUB_TOKEN`}),` authenticates only the API request;
it is not sent to the asset download. Toolchain mirrors, caches, download directories
and project files do not control self-update. Other platforms/build targets currently
have no self-update asset.`]}),`
`,(0,n.jsxs)(t.p,{children:[`Updating requires write access to the executable's directory. Staging is created beside
that executable, and a sibling `,(0,n.jsx)(t.code,{children:`<executable>.update.lock`}),` prevents simultaneous updates.
The lock file may remain after completion; do not delete it to bypass an active update.
The old executable is renamed to `,(0,n.jsx)(t.code,{children:`<executable>.outdated`}),`, retained while its process
exits, and removed by the next actual update. After verifying the new executable, if backup
deletion is blocked by file usage, Windows Restart Manager closes its users and retries.
Resolve any remaining error manually. These sibling names are reserved for the updater.`]}),`
`,(0,n.jsxs)(t.p,{children:[`Replacement failure attempts to restore the original executable. The two renames are
not one atomic operation: if the process is terminated between them and the original
path is missing, rename `,(0,n.jsx)(t.code,{children:`<executable>.outdated`}),` back to the original name before retrying.
A reported rollback failure also gives the exact restore paths. Self-update does not
change registered toolchains or shell configuration.`]}),`
`,(0,n.jsxs)(t.p,{children:[`Self-update preserves the running executable architecture: x64 builds select the x64 asset; ARM64 builds select `,(0,n.jsx)(t.code,{children:`hanamori-aarch64-pc-windows-msvc.exe`}),`, without switching according to OS architecture.`]})]})}function s(e={}){let{wrapper:t}=e.components||{};return t?(0,n.jsx)(t,{...e,children:(0,n.jsx)(o,{...e})}):o(e)}export{s as default,r as frontmatter,i as structuredData,a as toc};