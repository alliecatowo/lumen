import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const lumenGrammarPath = new URL(
  "../../editors/vscode/syntaxes/lumen.tmLanguage.json",
  import.meta.url,
);

const lumenGrammar = JSON.parse(
  readFileSync(fileURLToPath(lumenGrammarPath), "utf8"),
);

const ebnfGrammar = {
  name: "ebnf",
  scopeName: "source.ebnf",
  patterns: [
    { include: "#comments" },
    { include: "#strings" },
    { include: "#nonterminal" },
    { include: "#terminal" },
    { include: "#operators" },
    { include: "#punctuation" },
  ],
  repository: {
    comments: {
      patterns: [
        { name: "comment.line.double-slash.ebnf", match: "//.*$" },
        { name: "comment.line.number-sign.ebnf", match: "#.*$" },
      ],
    },
    strings: {
      patterns: [
        {
          name: "string.quoted.double.ebnf",
          begin: "\"",
          end: "\"",
          patterns: [{ name: "constant.character.escape.ebnf", match: "\\\\." }],
        },
        {
          name: "string.quoted.single.ebnf",
          begin: "'",
          end: "'",
          patterns: [{ name: "constant.character.escape.ebnf", match: "\\\\." }],
        },
      ],
    },
    nonterminal: {
      patterns: [
        { name: "entity.name.type.nonterminal.ebnf", match: "<[^>\\n]+>" },
        {
          name: "entity.name.type.nonterminal.ebnf",
          match: "\\b[A-Za-z_][A-Za-z0-9_-]*\\b(?=\\s*(?:::?=|=))",
        },
      ],
    },
    terminal: {
      patterns: [{ name: "constant.other.terminal.ebnf", match: "\\b[A-Z][A-Z0-9_]*\\b" }],
    },
    operators: {
      patterns: [
        { name: "keyword.operator.definition.ebnf", match: "::=|:=|=" },
        { name: "keyword.operator.alternative.ebnf", match: "\\|" },
        { name: "keyword.operator.quantifier.ebnf", match: "[?*+]" },
      ],
    },
    punctuation: {
      patterns: [
        { name: "punctuation.section.group.begin.ebnf", match: "[\\[{(]" },
        { name: "punctuation.section.group.end.ebnf", match: "[\\]})]" },
        { name: "punctuation.separator.sequence.ebnf", match: "[,;]" },
      ],
    },
  },
};

export const shikiLanguages = [
  {
    ...lumenGrammar,
    name: "lumen",
  },
  ebnfGrammar,
];
