import { defineConfig } from "vitepress";
import { shikiLanguages } from "./shiki-languages";

export default defineConfig({
  title: "Lumen",
  description:
    "A markdown-native, statically typed language for AI-native systems.",
  base: "/",
  cleanUrls: true,
  lastUpdated: true,
  srcDir: "docs",
  markdown: {
    languages: shikiLanguages,
  },
  head: [
    ["meta", { name: "theme-color", content: "#FF4FA3" }],
    ["link", { rel: "icon", type: "image/svg+xml", href: "/favicon.svg" }],
  ],
  themeConfig: {
    siteTitle: "Lumen",
    nav: [
      { text: "Guide", link: "/guide/effects" },
      { text: "Reference", link: "/reference/builtins" },
      { text: "GitHub", link: "https://github.com/alliecatowo/lumen" },
    ],
    search: {
      provider: "local",
    },
    socialLinks: [
      { icon: "github", link: "https://github.com/alliecatowo/lumen" },
    ],
    sidebar: {
      "/guide/": [
        {
          text: "Guides",
          items: [
            { text: "Algebraic Effects", link: "/guide/effects" },
            { text: "Architecture Overview", link: "/guide/architecture" },
            { text: "Editor Setup", link: "/guide/editor-setup" },
          ],
        },
      ],
      "/reference/": [
        {
          text: "Reference",
          items: [
            { text: "Builtins", link: "/reference/builtins" },
          ],
        },
      ],
    },
    footer: {
      message: "MIT Licensed",
      copyright: "Copyright 2026 Lumen contributors",
    },
    editLink: {
      pattern: "https://github.com/alliecatowo/lumen/edit/main/wares-site/docs/:path",
      text: "Edit this page on GitHub",
    },
  },
});
