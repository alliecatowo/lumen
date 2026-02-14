import DefaultTheme from "vitepress/theme";
import WasmPlayground from "./components/WasmPlayground.vue";
import "./custom.css";

export default {
  ...DefaultTheme,
  enhanceApp({ app }) {
    app.component("WasmPlayground", WasmPlayground);
  },
};
