//! ANSI color helpers for CLI output.

/// Format text in green.
pub fn green(s: &str) -> String {
    format!("\x1b[32m{}\x1b[0m", s)
}

/// Format text in red.
pub fn red(s: &str) -> String {
    format!("\x1b[31m{}\x1b[0m", s)
}

/// Format text in yellow.
pub fn yellow(s: &str) -> String {
    format!("\x1b[33m{}\x1b[0m", s)
}

/// Format text in cyan.
pub fn cyan(s: &str) -> String {
    format!("\x1b[36m{}\x1b[0m", s)
}

/// Format text in bold.
pub fn bold(s: &str) -> String {
    format!("\x1b[1m{}\x1b[0m", s)
}

/// Format text in gray.
pub fn gray(s: &str) -> String {
    format!("\x1b[90m{}\x1b[0m", s)
}

/// Format a status label (right-aligned, green, bold).
pub fn status_label(label: &str) -> String {
    format!("\x1b[1;32m{:>12}\x1b[0m", label)
}
