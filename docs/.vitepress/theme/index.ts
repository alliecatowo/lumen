import { h } from 'vue';
import DefaultTheme from "vitepress/theme";
import WasmPlayground from "./components/WasmPlayground.vue";
import CodeBlockRunner from "./components/CodeBlockRunner.vue";
import "./custom.css";

export default {
  ...DefaultTheme,
  Layout: () => h(DefaultTheme.Layout, null, {
    'doc-after': () => h(CodeBlockRunner),
  }),
  enhanceApp({ app }) {
    app.component("WasmPlayground", WasmPlayground);
    app.component("CodeBlockRunner", CodeBlockRunner);
  },
};
