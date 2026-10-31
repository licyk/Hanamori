import { createRootRoute, HeadContent, Outlet, Scripts } from "@tanstack/react-router";

import favicon from "@/assets/favicon.svg?url&no-inline";
import { SiteProvider } from "@/components/site-provider";
import { DEFAULT_LOCALE } from "@/lib/i18n";
import appCss from "@/styles/app.css?url";

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1" },
      {
        title: "Hanamori",
      },
      {
        name: "description",
        content: "Hanamori：独立安装、选择和管理 MSVC 与 Windows SDK。",
      },
    ],
    links: [
      { rel: "stylesheet", href: appCss },
      {
        rel: "icon",
        type: "image/svg+xml",
        href: favicon,
      },
    ],
  }),
  component: RootComponent,
});

function RootComponent() {
  return (
    <html lang={DEFAULT_LOCALE} suppressHydrationWarning>
      <head>
        <HeadContent />
      </head>
      <body className="flex min-h-screen flex-col">
        <SiteProvider>
          <Outlet />
        </SiteProvider>
        <Scripts />
      </body>
    </html>
  );
}
