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
  ignoreDeadLinks: [/\.\.\/SPEC/],
  head: [
    ["meta", { name: "theme-color", content: "#0f766e" }],
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
            { text: "Memory & Machines", link: "/learn/advanced/processes" },
            { text: "WASM Deployment", link: "/learn/advanced/wasm" },
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
          text: "Declarations",
          items: [
            { text: "Records", link: "/reference/records" },
            { text: "Enums", link: "/reference/enums" },
            { text: "Cells (Functions)", link: "/reference/cells" },
            { text: "Agents", link: "/reference/agents" },
            { text: "Effects & Handlers", link: "/reference/effects" },
            { text: "Traits & Impls", link: "/reference/traits" },
          ],
        },
        {
          text: "AI Constructs",
          items: [
            { text: "Tools", link: "/reference/tools" },
            { text: "Grants & Policies", link: "/reference/grants" },
            { text: "Processes", link: "/reference/processes" },
            { text: "Pipelines", link: "/reference/pipelines" },
            { text: "Memory", link: "/reference/memory" },
            { text: "Machines", link: "/reference/machines" },
          ],
        },
        {
          text: "Directives",
          items: [
            { text: "@strict", link: "/reference/directives/strict" },
            { text: "@deterministic", link: "/reference/directives/deterministic" },
            { text: "@doc_mode", link: "/reference/directives/doc-mode" },
          ],
        },
        {
          text: "Appendix",
          items: [
            { text: "Grammar (EBNF)", link: "/reference/grammar" },
            { text: "Keywords", link: "/reference/keywords" },
            { text: "Operator Precedence", link: "/reference/operators" },
          ],
        },
      ],
      "/api/": [
        {
          text: "Standard Library",
          items: [
            { text: "Overview", link: "/api/overview" },
            { text: "Builtins", link: "/api/builtins" },
          ],
        },
        {
          text: "Collections",
          items: [
            { text: "list", link: "/api/list" },
            { text: "map", link: "/api/map" },
            { text: "set", link: "/api/set" },
            { text: "tuple", link: "/api/tuple" },
          ],
        },
        {
          text: "Types",
          items: [
            { text: "result", link: "/api/result" },
            { text: "Json", link: "/api/json" },
            { text: "Bytes", link: "/api/bytes" },
          ],
        },
        {
          text: "String",
          items: [
            { text: "String Operations", link: "/api/string" },
          ],
        },
        {
          text: "Async",
          items: [
            { text: "Future", link: "/api/future" },
            { text: "Orchestration", link: "/api/orchestration" },
          ],
        },
      ],
      "/examples/": [
        {
          text: "Basics",
          items: [
            { text: "Hello World", link: "/examples/hello-world" },
            { text: "Calculator", link: "/examples/calculator" },
            { text: "Fibonacci", link: "/examples/fibonacci" },
            { text: "String Utils", link: "/examples/string-utils" },
          ],
        },
        {
          text: "Data Structures",
          items: [
            { text: "Linked List", link: "/examples/linked-list" },
            { text: "Sorting", link: "/examples/sorting" },
            { text: "Data Pipeline", link: "/examples/data-pipeline" },
          ],
        },
        {
          text: "AI-Native",
          items: [
            { text: "AI Chat", link: "/examples/ai-chat" },
            { text: "Code Reviewer", link: "/examples/code-reviewer" },
            { text: "Invoice Agent", link: "/examples/invoice-agent" },
            { text: "State Machine", link: "/examples/state-machine" },
            { text: "Task Tracker", link: "/examples/task-tracker" },
          ],
        },
        {
          text: "Advanced",
          items: [
            { text: "WASM Browser", link: "/examples/wasm-browser" },
            { text: "Syntax Sugar", link: "/examples/syntax-sugar" },
            { text: "Language Features", link: "/examples/language-features" },
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
            { text: "MCP Integration", link: "/guide/mcp" },
            { text: "Package Management", link: "/guide/packages" },
          ],
        },
        {
          text: "Internal",
          items: [
            { text: "Architecture", link: "/guide/architecture" },
            { text: "Runtime Model", link: "/guide/runtime" },
            { text: "LIR Bytecode", link: "/guide/lir" },
          ],
        },
      ],
    },
    footer: {
      message: "MIT Licensed",
      copyright: "Copyright © 2026 Lumen contributors",
    },
    editLink: {
      pattern: "https://github.com/alliecatowo/lumen/edit/main/docs/:path",
      text: "Edit this page on GitHub",
    },
  },
});
