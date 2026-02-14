import { defineConfig } from "vitepress";

const repository = process.env.GITHUB_REPOSITORY?.split("/")[1] ?? "lumen";
const base = process.env.CI ? `/${repository}/` : "/";

export default defineConfig({
  title: "Lumen",
  description:
    "A markdown-native, statically typed language for AI-native systems.",
  base,
  cleanUrls: true,
  lastUpdated: true,
  ignoreDeadLinks: [/\.\.\/SPEC/],
  head: [
    ["meta", { name: "theme-color", content: "#0f766e" }],
    ["meta", { property: "og:type", content: "website" }],
    ["meta", { property: "og:title", content: "Lumen Language Docs" }],
    [
      "meta",
      {
        property: "og:description",
        content:
          "Documentation for Lumen: language guides, AI-native features, runtime, and CLI reference.",
      },
    ],
  ],
  markdown: {
    config(md) {
      const fallbackFence = md.renderer.rules.fence;
      if (!fallbackFence) {
        return;
      }

      md.renderer.rules.fence = (tokens, idx, options, env, self) => {
        const token = tokens[idx];
        const lang = token.info.trim().split(/\s+/)[0];
        if (lang === "lumen" || lang === "ebnf") {
          token.info = token.info.replace(lang, "txt");
        }
        return fallbackFence(tokens, idx, options, env, self);
      };
    },
  },
  themeConfig: {
    siteTitle: "Lumen",
    logo: "/logo.svg",
    nav: [
      { text: "Guide", link: "/guide/quickstart" },
      { text: "Language", link: "/language/tour" },
      { text: "Reference", link: "/CLI" },
      { text: "GitHub", link: "https://github.com/alliecatowo/lumen" },
    ],
    search: {
      provider: "local",
    },
    socialLinks: [
      { icon: "github", link: "https://github.com/alliecatowo/lumen" },
    ],
    sidebar: [
      {
        text: "Guide",
        items: [
          { text: "Quickstart", link: "/guide/quickstart" },
          { text: "Documentation Map", link: "/guide/docs-map" },
        ],
      },
      {
        text: "Language",
        items: [
          { text: "Language Tour", link: "/language/tour" },
          { text: "AI-Native Features", link: "/language/ai-native" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "CLI", link: "/CLI" },
          { text: "Runtime", link: "/RUNTIME" },
          { text: "Architecture", link: "/ARCHITECTURE" },
          { text: "Getting Started (Legacy Doc)", link: "/GETTING_STARTED" },
        ],
      },
      {
        text: "Research",
        items: [
          { text: "WASM Strategy", link: "/WASM_STRATEGY" },
          { text: "Tooling Gaps", link: "/TOOLING_GAPS" },
          { text: "Competitive Analysis", link: "/COMPETITIVE_ANALYSIS" },
        ],
      },
    ],
    footer: {
      message: "MIT Licensed",
      copyright: "Copyright © 2026 Lumen contributors",
    },
  },
});
