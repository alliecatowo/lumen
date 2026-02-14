import { defineConfig } from "vitepress";
import { shikiLanguages } from "./shiki-languages";

const repository = process.env.GITHUB_REPOSITORY?.split("/")[1] ?? "lumen";
const base = process.env.CI ? `/${repository}/` : "/";
const iconHref = `${base}logo.svg`;

export default defineConfig({
  title: "Lumen",
  description:
    "A markdown-native, statically typed language for AI-native systems.",
  base,
  cleanUrls: true,
  lastUpdated: true,
  ignoreDeadLinks: true,
  head: [
    ["meta", { name: "theme-color", content: "#FF4FA3" }],
    ["link", { rel: "icon", type: "image/svg+xml", href: iconHref }],
    ["link", { rel: "apple-touch-icon", href: iconHref }],
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
    languages: shikiLanguages,
  },
  vite: {
    optimizeDeps: {
      exclude: ["/wasm/lumen_wasm.js"],
    },
    build: {
      target: "esnext",
    },
  },
  themeConfig: {
    siteTitle: "Lumen",
    logo: "/logo.svg",
    nav: [
      { text: "Learn", link: "/learn/getting-started" },
      { text: "Language", link: "/reference/overview" },
      { text: "API", link: "/api/builtins" },
      { text: "Examples", link: "/examples/hello-world" },
      { text: "Playground", link: "/playground" },
      { text: "GitHub", link: "https://github.com/alliecatowo/lumen" },
    ],
    search: {
      provider: "local",
    },
    socialLinks: [
      { icon: "github", link: "https://github.com/alliecatowo/lumen" },
    ],
    sidebar: {
      "/learn/": [
        {
          text: "Getting Started",
          items: [
            { text: "Introduction", link: "/learn/introduction" },
            { text: "Installation", link: "/learn/installation" },
            { text: "Quick Start", link: "/learn/getting-started" },
            { text: "First Program", link: "/learn/first-program" },
          ],
        },
        {
          text: "Tutorial",
          items: [
            { text: "Basics", link: "/learn/tutorial/basics" },
            { text: "Control Flow", link: "/learn/tutorial/control-flow" },
            { text: "Data Structures", link: "/learn/tutorial/data-structures" },
            { text: "Functions", link: "/learn/tutorial/functions" },
            { text: "Pattern Matching", link: "/learn/tutorial/pattern-matching" },
            { text: "Error Handling", link: "/learn/tutorial/error-handling" },
          ],
        },
        {
          text: "AI-Native Tutorial",
          items: [
            { text: "Tools & Grants", link: "/learn/ai-native/tools" },
            { text: "Agents", link: "/learn/ai-native/agents" },
            { text: "Processes", link: "/learn/ai-native/processes" },
            { text: "Pipelines", link: "/learn/ai-native/pipelines" },
            { text: "Orchestration", link: "/learn/ai-native/orchestration" },
          ],
        },
        {
          text: "Advanced",
          items: [
            { text: "Effects System", link: "/learn/advanced/effects" },
            { text: "Async & Futures", link: "/learn/advanced/async" },
            { text: "WASM Deployment", link: "/guide/wasm-browser" },
          ],
        },
      ],
      "/reference/": [
        {
          text: "Language Reference",
          items: [
            { text: "Overview", link: "/reference/overview" },
            { text: "Source Model", link: "/reference/source-model" },
            { text: "Types", link: "/reference/types" },
            { text: "Expressions", link: "/reference/expressions" },
            { text: "Statements", link: "/reference/statements" },
            { text: "Pattern Matching", link: "/reference/patterns" },
            { text: "Declarations", link: "/reference/declarations" },
          ],
        },
        {
          text: "AI Constructs",
          items: [
            { text: "Tools", link: "/reference/tools" },
            { text: "Grants & Policies", link: "/reference/grants" },
          ],
        },
        {
          text: "Directives",
          items: [
            { text: "@strict", link: "/reference/directives/strict" },
            { text: "@deterministic", link: "/reference/directives/deterministic" },
          ],
        },
        {
          text: "Appendix",
          items: [
            { text: "Grammar (EBNF)", link: "/reference/grammar" },
          ],
        },
      ],
      "/api/": [
        {
          text: "Standard Library",
          items: [
            { text: "Builtins", link: "/api/builtins" },
          ],
        },
      ],
      "/examples/": [
        {
          text: "Basics",
          items: [
            { text: "Hello World", link: "/examples/hello-world" },
          ],
        },
        {
          text: "AI-Native",
          items: [
            { text: "AI Chat", link: "/examples/ai-chat" },
          ],
        },
      ],
      "/guide/": [
        {
          text: "Guides",
          items: [
            { text: "CLI Reference", link: "/guide/cli" },
            { text: "Configuration", link: "/guide/configuration" },
            { text: "Tool Providers", link: "/guide/providers" },
          ],
        },
      ],
    },
    footer: {
      message: "MIT Licensed",
      copyright: "Copyright 2026 Lumen contributors",
    },
    editLink: {
      pattern: "https://github.com/alliecatowo/lumen/edit/main/docs/:path",
      text: "Edit this page on GitHub",
    },
  },
});
