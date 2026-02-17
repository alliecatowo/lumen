# std.compiler.span — Source Locations and Diagnostics

Core types for tracking source positions, representing source files,
and reporting compiler diagnostics.

```lumen
# ── Source location ──────────────────────────────────────────────

# Byte-level span in source text.
# start/end_pos are byte offsets; line/col are 1-based.
record Span(
  file: String,
  start: Int,
  end_pos: Int,
  start_line: Int,
  start_col: Int
)

# Create a dummy span (used in generated/synthetic nodes).
cell dummy_span() -> Span
  return Span(file: "", start: 0, end_pos: 0, start_line: 0, start_col: 0)
end

# Merge two spans into the smallest span covering both.
cell merge_spans(a: Span, b: Span) -> Span
  let start = min(a.start, b.start)
  let end_pos = max(a.end_pos, b.end_pos)
  let line = min(a.start_line, b.start_line)
  let col = 0
  if a.start_line <= b.start_line
    col = a.start_col
  else
    col = b.start_col
  end
  return Span(file: a.file, start: start, end_pos: end_pos, start_line: line, start_col: col)
end

# ── Source file ──────────────────────────────────────────────────

# A loaded source file with its content and line-start offsets.
record Source(
  filename: String,
  content: String,
  lines: list[Int]
)

# Build a Source from filename and raw content string.
# Populates the line-start offset table.
cell make_source(filename: String, content: String) -> Source
  let lines = [0]
  for i in range(0, length(content)) if slice(content, i, i + 1) == "\n"
    lines = append(lines, i + 1)
  end
  return Source(filename: filename, content: content, lines: lines)
end

# ── Diagnostics ──────────────────────────────────────────────────

enum DiagnosticLevel
  Error
  Warning
  Note
  Help
end

# A single compiler diagnostic with location, message, and optional notes.
record Diagnostic(
  level: DiagnosticLevel,
  message: String,
  span: Span,
  notes: list[String]
)

# Format a diagnostic as a human-readable string.
cell format_diagnostic(diag: Diagnostic, src: Source) -> String
  let prefix = match diag.level
    case Error -> "error"
    case Warning -> "warning"
    case Note -> "note"
    case Help -> "help"
  end

  let loc = "{src.filename}:{diag.span.start_line}:{diag.span.start_col}"
  let header = "{prefix}: {diag.message}\n  --> {loc}"

  let result = header
  for note in diag.notes
    result = result ++ "\n  = note: {note}"
  end
  return result
end

# Convenience constructors for common diagnostic levels.
cell error_at(span: Span, message: String) -> Diagnostic
  return Diagnostic(level: Error, message: message, span: span, notes: [])
end

cell warning_at(span: Span, message: String) -> Diagnostic
  return Diagnostic(level: Warning, message: message, span: span, notes: [])
end

cell note_at(span: Span, message: String) -> Diagnostic
  return Diagnostic(level: Note, message: message, span: span, notes: [])
end
```
