import { StreamLanguage, StringStream } from "@codemirror/language";
import { tags } from "@lezer/highlight";

const keywords = new Set([
  "cell", "end", "if", "else", "match", "for", "while", "return",
  "let", "enum", "record", "process", "effect", "handler", "grant",
  "use", "tool", "import", "async", "await", "try", "catch", "emit",
  "break", "continue", "fn", "in", "and", "or", "not", "is", "as",
  "when", "then", "bind", "with", "from", "where",
]);

const constants = new Set(["true", "false", "null"]);

const typeNames = new Set([
  "Int", "Float", "String", "Bool", "Null", "Json", "Bytes",
  "list", "map", "set", "tuple", "result", "Any",
]);

const builtins = new Set([
  "len", "push", "pop", "append", "join", "split", "trim",
  "contains", "keys", "values", "to_string", "to_int", "to_float",
  "print", "range", "sort", "filter", "reduce", "ok", "err",
  "parallel", "race", "vote", "select", "timeout",
]);

interface LumenState {
  inString: boolean;
  inInterpolation: number;
}

const lumenStreamParser = {
  name: "lumen",
  startState(): LumenState {
    return { inString: false, inInterpolation: 0 };
  },
  token(stream: StringStream, state: LumenState): string | null {
    // Inside string interpolation
    if (state.inInterpolation > 0 && !state.inString) {
      if (stream.peek() === "}") {
        state.inInterpolation--;
        stream.next();
        return "string";
      }
      // Tokenize interpolation contents normally
      return tokenNormal(stream, state);
    }

    // Inside a string
    if (state.inString) {
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === "\\") {
          stream.next(); // skip escaped char
        } else if (ch === "{") {
          state.inInterpolation++;
          state.inString = false;
          return "string";
        } else if (ch === '"') {
          state.inString = false;
          return "string";
        }
      }
      return "string";
    }

    return tokenNormal(stream, state);
  },
  indent(): null {
    return null;
  },
};

function tokenNormal(stream: StringStream, state: LumenState): string | null {
  // Whitespace
  if (stream.eatSpace()) return null;

  // Comment
  if (stream.match("#")) {
    stream.skipToEnd();
    return "lineComment";
  }

  // String
  if (stream.peek() === '"') {
    stream.next();
    state.inString = true;
    // Read until end of string or interpolation
    while (!stream.eol()) {
      const ch = stream.next();
      if (ch === "\\") {
        stream.next();
      } else if (ch === "{") {
        state.inInterpolation++;
        state.inString = false;
        return "string";
      } else if (ch === '"') {
        state.inString = false;
        return "string";
      }
    }
    return "string";
  }

  // Numbers
  if (stream.match(/^0x[0-9a-fA-F]+/) ||
      stream.match(/^0b[01]+/) ||
      stream.match(/^0o[0-7]+/) ||
      stream.match(/^\d+\.\d+([eE][+-]?\d+)?/) ||
      stream.match(/^\d+([eE][+-]?\d+)?/)) {
    return "number";
  }

  // Multi-char operators (check before single char)
  if (stream.match("->") || stream.match("|>") ||
      stream.match("=>") || stream.match("..=") || stream.match("..") ||
      stream.match("?.") || stream.match("??") ||
      stream.match("==") || stream.match("!=") ||
      stream.match("<=") || stream.match(">=") ||
      stream.match("<<") || stream.match(">>") ||
      stream.match("**")) {
    return "operator";
  }

  // Single-char operators and punctuation
  const ch = stream.peek();
  if (ch && "+-*/%=<>!&|^~".includes(ch)) {
    stream.next();
    return "operator";
  }

  if (ch && "()[]{},:;.@".includes(ch)) {
    stream.next();
    return "punctuation";
  }

  // Identifiers and keywords
  if (stream.match(/^[a-zA-Z_][a-zA-Z0-9_]*/)) {
    const word = stream.current();
    if (keywords.has(word)) return "keyword";
    if (constants.has(word)) return "atom";
    if (typeNames.has(word)) return "typeName";
    if (builtins.has(word)) return "variableName.standard";
    // Uppercase start = type name
    if (word[0] >= "A" && word[0] <= "Z") return "typeName";
    return "variableName";
  }

  // Skip unknown
  stream.next();
  return null;
}

export const lumenLanguage = StreamLanguage.define(lumenStreamParser);

// Theme token mapping for CodeMirror tags
import { HighlightStyle } from "@codemirror/language";

export const lumenHighlightStyle = HighlightStyle.define([
  { tag: tags.keyword, color: "#FF4FA3", fontWeight: "bold" },
  { tag: tags.atom, color: "#d19a66" },
  { tag: tags.number, color: "#d19a66" },
  { tag: tags.string, color: "#98c379" },
  { tag: tags.lineComment, color: "#6b7280", fontStyle: "italic" },
  { tag: tags.typeName, color: "#e5c07b" },
  { tag: tags.variableName, color: "#e0e0e0" },
  { tag: tags.standard(tags.variableName), color: "#61afef" },
  { tag: tags.operator, color: "#c678dd" },
  { tag: tags.punctuation, color: "#abb2bf" },
]);
