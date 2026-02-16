import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const lumenGrammarPath = new URL(
  "../../editors/vscode/syntaxes/lumen.tmLanguage.json",
  import.meta.url,
);

const lumenGrammar = JSON.parse(
  readFileSync(fileURLToPath(lumenGrammarPath), "utf8"),
);

export const shikiLanguages = [
  {
    ...lumenGrammar,
    name: "lumen",
  },
];
