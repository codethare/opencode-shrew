use chrono::DateTime;
use std::sync::OnceLock;

use regex::Regex;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;
use unicode_width::UnicodeWidthStr;

pub mod markdown;
pub mod views;

// Re-export public items from submodules
pub use markdown::render_markdown;
pub use views::*;

// ── Timestamp helpers ──

fn ts_to_iso(ts: i64) -> String {
    let secs = ts / 1000;
    let nsecs = (ts.rem_euclid(1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => format!("(invalid timestamp {})", ts),
    }
}

fn ts_to_iso_short(ts: i64) -> String {
    let secs = ts / 1000;
    let nsecs = (ts.rem_euclid(1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => format!("(invalid timestamp {})", ts),
    }
}

pub(crate) fn ts_to_compact(ts: i64) -> String {
    let secs = ts / 1000;
    let nsecs = (ts.rem_euclid(1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%m-%d %H:%M").to_string(),
        None => String::new(),
    }
}

fn ts_to_time(ts: i64) -> String {
    let secs = ts / 1000;
    let nsecs = (ts.rem_euclid(1000) * 1_000_000) as u32;
    match DateTime::from_timestamp(secs, nsecs) {
        Some(dt) => dt.format("%H:%M:%S").to_string(),
        None => String::new(),
    }
}

// ── Highlighting resources ──

struct HighlightResources {
    ss: SyntaxSet,
    ts: ThemeSet,
}

fn highlight_resources() -> &'static HighlightResources {
    static RES: OnceLock<HighlightResources> = OnceLock::new();
    RES.get_or_init(|| HighlightResources {
        ss: SyntaxSet::load_defaults_newlines(),
        ts: ThemeSet::load_defaults(),
    })
}

// ── Text utilities ──

/// Strip ANSI escape codes from text (e.g. from subprocess output).
pub fn strip_ansi(text: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap());
    re.replace_all(text, "").into_owned()
}

/// Compute the visible terminal width of a string after stripping ANSI codes.
/// Uses [`UnicodeWidthStr::width`] so that CJK characters (width=2) and
/// combining characters (width=0) are handled correctly.
pub fn visible_width(text: &str) -> usize {
    UnicodeWidthStr::width(strip_ansi(text).as_str())
}

// ── Code highlighting ──

/// Highlight a raw code snippet with syntect, given a language hint.
fn highlight_snippet(code: &str, lang: &str) -> String {
    const HIGHLIGHT_MAX_LINES: usize = 100;
    let line_count = code.lines().count();
    if line_count > HIGHLIGHT_MAX_LINES {
        return format!("\x1b[1m\x1b[38;5;245m┌─ {lang} ({} lines, highlighting skipped)\x1b[0m\n{code}\n\x1b[1m\x1b[38;5;245m└─\x1b[0m",
            line_count);
    }

    let res = highlight_resources();
    let theme = &res.ts.themes["base16-ocean.dark"];
    let syntax = res
        .ss
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| res.ss.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut result = String::new();
    for line in code.lines() {
        if let Ok(ranges) = highlighter.highlight_line(line, &res.ss) {
            result.push_str(&as_24_bit_terminal_escaped(&ranges, false));
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    result
}
