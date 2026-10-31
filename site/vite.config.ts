import react from "@vitejs/plugin-react";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import tailwindcss from "@tailwindcss/vite";
import { fumadocsMdx } from "fumadocs-mdx/vite";
import { nitro } from "nitro/vite";
import { defineConfig } from "vite";

export default defineConfig({
  base: process.env.VITE_BASE_PATH || "/",
  server: {
    host: "127.0.0.1",
    port: 3000,
    strictPort: true,
  },
  preview: {
    host: "127.0.0.1",
    port: 3000,
    strictPort: true,
  },
  plugins: [
    {
      name: "static-server-function-base-path",
      transform(code, id) {
        if (!id.replaceAll("\\", "/").endsWith("/start-static-server-functions/dist/esm/staticFunctionMiddleware.js")) return;
        // This pinned middleware fetches its cache from /__tsr even on project sites.
        // Prefix client requests only; the emitted cache stays at the output root.
        return {
          code: code.replace(
            "return fetch(url, {",
            'return fetch(import.meta.env.BASE_URL.replace(/\\/$/, "") + url, {',
          ),
          map: null,
        };
      },
    },
    fumadocsMdx(),
    tailwindcss(),
    tanstackStart({
      spa: {
        enabled: true,
        maskPath: "/spa-shell",
        prerender: {
          enabled: true,
          crawlLinks: true,
        },
      },
      pages: [{ path: "/" }, { path: "/download" }, { path: "/docs" }, { path: "/api/search" }],
    }),
    react(),
    nitro({
      preset: "node-server",
    }),
  ],
  resolve: {
    tsconfigPaths: true,
  },
});
