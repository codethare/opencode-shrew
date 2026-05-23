use regex::Regex;

use super::{
    highlight_snippet, visible_width,
};

use pulldown_cmark as pd;

/// Render markdown text to ANSI-terminal-formatted output.
///
/// Parses GFM markdown via `pulldown-cmark` and produces a richly formatted
/// terminal string with syntax-highlighted code blocks, styled tables, lists,
/// links, blockquotes, etc. Replaces the previous `apply_terminal_styles()` +
/// `highlight_code_blocks()` two-step pipeline.
fn preprocess_markdown(text: &str) -> String {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?s)<details>\n?<summary>(.*?)</summary>\s*(.*?)</details>")
            .expect("invalid details regex")
    });
    re.replace_all(text, |caps: &regex::Captures| {
        format!("**▼ {}**\n\n{}", &caps[1], &caps[2])
    })
    .into_owned()
}

/// Render markdown text to ANSI-terminal-formatted output.
///
/// Parses GFM markdown via `pulldown-cmark` and produces a richly formatted
/// terminal string with syntax-highlighted code blocks, styled tables, lists,
/// links, blockquotes, etc. Replaces the previous `apply_terminal_styles()` +
/// `highlight_code_blocks()` two-step pipeline.
pub fn render_markdown(text: &str) -> String {
    let cleaned = preprocess_markdown(text);
    let parser = pd::Parser::new_ext(&cleaned, pd::Options::all());
    let mut r = MdRenderer::new();
    for event in parser {
        match event {
            pd::Event::Start(tag) => r.start(&tag),
            pd::Event::End(tag) => r.end(&tag),
            pd::Event::Text(t) => r.text(&t),
            pd::Event::Code(t) => r.code(&t),
            pd::Event::SoftBreak | pd::Event::HardBreak => r.out("\n"),
            pd::Event::Rule => r.out("\x1b[2m────────────────────────────────────────────────────────────\x1b[0m\n"),
            pd::Event::TaskListMarker(checked) => {
                r.out(if checked { "\x1b[92m☑\x1b[0m " } else { "\x1b[2m☐\x1b[0m " });
            }
            // If any inline HTML slips through, render it dimmed
            pd::Event::Html(html) => {
                // Strip tags for cleaner display, render dimmed
                let stripped = html.replace('<', "").replace('>', "");
                if !stripped.trim().is_empty() {
                    r.out(&format!("\x1b[2m{}\x1b[0m", stripped));
                }
            }
            pd::Event::InlineHtml(html) => {
                if !html.trim().is_empty() {
                    r.out(&format!("\x1b[2m{}\x1b[0m", html));
                }
            }
            _ => {}
        }
    }
    r.finish()
}

struct MdRenderer {
    output: String,
    // saved outputs for buffering (blockquotes, table cells)
    saved: Vec<String>,
    // heading
    heading_level: u32,
    // list
    list_ordered: Vec<bool>,
    list_indexes: Vec<u64>,
    // code block
    code_buf: String,
    code_lang: String,
    // table
    tbl_aligns: Vec<pd::Alignment>,
    tbl_rows: Vec<Vec<String>>,
    tbl_row: Vec<String>,
    tbl_cell: String,
    tbl_in_head: bool,
    // link footnotes
    link_links: Vec<(String, String)>, // (text, url)
    // blockquote
    quote_depth: usize,
}

impl MdRenderer {
    fn new() -> Self {
        Self {
            output: String::new(),
            saved: Vec::new(),
            heading_level: 0,
            list_ordered: Vec::new(),
            list_indexes: Vec::new(),
            code_buf: String::new(),
            code_lang: String::new(),
            tbl_aligns: Vec::new(),
            tbl_rows: Vec::new(),
            tbl_row: Vec::new(),
            tbl_cell: String::new(),
            tbl_in_head: false,
            link_links: Vec::new(),
            quote_depth: 0,
        }
    }

    fn out(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn start(&mut self, tag: &pd::Tag) {
        match tag {
            pd::Tag::Heading { level, .. } => {
                self.heading_level = *level as u32;
                match *level as u32 {
                    1 => self.out("\x1b[1m"),
                    2 => self.out("\x1b[92m\x1b[1m"),
                    _ => self.out("\x1b[94m\x1b[1m"),
                }
            }
            pd::Tag::Paragraph => {
                // If inside a blockquote, paragraphs don't get extra spacing
            }
            pd::Tag::Emphasis => self.out("\x1b[3m"),
            pd::Tag::Strong => self.out("\x1b[1m"),
            pd::Tag::Strikethrough => self.out("\x1b[9m"),
            pd::Tag::Link { dest_url, .. } => {
                self.link_links.push((String::new(), dest_url.to_string()));
                self.out("\x1b[4m\x1b[34m");
            }
            pd::Tag::Image { dest_url, .. } => {
                self.out(&format!("\x1b[2m[{}]\x1b[0m", dest_url));
            }
            pd::Tag::List(opt) => {
                self.list_ordered.push(opt.is_some());
                self.list_indexes.push(opt.unwrap_or(1));
            }
            pd::Tag::Item => {
                if let Some(&ordered) = self.list_ordered.last() {
                    let level = self.list_ordered.len();
                    let indent = "  ".repeat(level - 1);
                    if ordered {
                        let n = {
                            let idx = self.list_indexes.last_mut().unwrap();
                            let n = *idx;
                            *idx += 1;
                            n
                        };
                        self.out(&format!("{indent}{n}.\x1b[0m "));
                    } else {
                        self.out(&format!("{indent}\x1b[37m•\x1b[0m "));
                    }
                }
            }
            pd::Tag::CodeBlock(kind) => {
                self.code_lang = match kind {
                    pd::CodeBlockKind::Fenced(info) => info.to_string(),
                    pd::CodeBlockKind::Indented => String::new(),
                };
                self.code_buf.clear();
            }
            pd::Tag::Table(aligns) => {
                self.tbl_aligns = aligns.clone();
                self.tbl_rows.clear();
                self.tbl_in_head = false;
            }
            pd::Tag::TableHead => {
                self.tbl_in_head = true;
            }
            pd::Tag::TableRow => {
                self.tbl_row.clear();
            }
            pd::Tag::TableCell => {
                self.tbl_cell.clear();
                let prev = std::mem::take(&mut self.output);
                self.saved.push(prev);
            }
            pd::Tag::BlockQuote(_) => {
                let prev = std::mem::take(&mut self.output);
                self.saved.push(prev);
                self.quote_depth += 1;
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: &pd::TagEnd) {
        match tag {
            pd::TagEnd::Heading(_) => {
                self.out("\x1b[0m\n");
                self.heading_level = 0;
            }
            pd::TagEnd::Paragraph => {
                // In list items, paragraphs are inline; in blockquotes,
                // spacing is handled by quote prefix logic
                let in_list = !self.list_ordered.is_empty();
                let in_quote = self.quote_depth > 0;
                if !in_list && !in_quote {
                    self.out("\n");
                }
            }
            pd::TagEnd::Emphasis => self.out("\x1b[23m"),
            pd::TagEnd::Strong => self.out("\x1b[22m"),
            pd::TagEnd::Strikethrough => self.out("\x1b[29m"),
            pd::TagEnd::Link => {
                let n = self.link_links.len();
                self.out(&format!("\x1b[0m\x1b[2m[{}]\x1b[0m", n));
            }
            pd::TagEnd::Image => {
                // Image alt text children were not rendered
            }
            pd::TagEnd::List(_tight) => {
                self.list_ordered.pop();
                self.list_indexes.pop();
                if self.list_ordered.is_empty() {
                    self.out("\n");
                }
            }
            pd::TagEnd::Item => {
                self.out("\n");
            }
            pd::TagEnd::CodeBlock => {
                // Highlight the collected code buffer
                let highlighted = highlight_snippet(&self.code_buf, &self.code_lang);
                self.out(&highlighted);
                self.out("\n");
            }
            pd::TagEnd::Table => {
                self.render_table();
            }
            pd::TagEnd::TableHead => {
                self.tbl_in_head = false;
            }
            pd::TagEnd::TableRow => {
                let row = std::mem::take(&mut self.tbl_row);
                self.tbl_rows.push(row);
            }
            pd::TagEnd::TableCell => {
                let cell = std::mem::replace(&mut self.output, self.saved.pop().unwrap());
                self.tbl_row.push(cell);
            }
            pd::TagEnd::BlockQuote(_) => {
                let content = std::mem::replace(&mut self.output, self.saved.pop().unwrap());
                for line in content.lines() {
                    self.out(&format!("\x1b[2m│\x1b[0m {}\n", line));
                }
                self.quote_depth -= 1;
            }
            _ => {}
        }
    }

    fn text(&mut self, t: &str) {
        if !self.code_buf.is_empty() || !self.code_lang.is_empty() {
            // In a code block — collect text for later highlighting
            self.code_buf.push_str(t);
            return;
        }
        let styled = style_data_text(t);
        self.out(&styled);
    }

    fn code(&mut self, t: &str) {
        self.out(&format!("\x1b[96m{}\x1b[0m", t));
    }

    fn finish(&mut self) -> String {
        // Append link footnotes
        for (i, &(_, ref url)) in self.link_links.iter().enumerate() {
            self.output.push_str(&format!("\x1b[2m [{}]: {}\x1b[0m\n", i + 1, url));
        }
        std::mem::take(&mut self.output)
    }

    fn render_table(&mut self) {
        let rows = std::mem::take(&mut self.tbl_rows);
        if rows.is_empty() {
            return;
        }
        let ncols = self.tbl_aligns.len().max(
            rows.iter().map(|r| r.len()).max().unwrap_or(0),
        );
        if ncols == 0 {
            return;
        }

        // Pad all rows to ncols
        let mut rows = rows;
        for row in &mut rows {
            while row.len() < ncols {
                row.push(String::new());
            }
        }

        let mut widths = vec![3usize; ncols];
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                let w = visible_width(cell);
                if w > widths[i] {
                    widths[i] = w;
                }
            }
        }
        for w in &mut widths {
            *w = (*w).max(3);
        }

        // Top border
        self.top_border(&widths);
        self.out("\n");

        if !rows.is_empty() {
            self.data_row(&rows[0], &widths);
            self.sep_border(&widths);
            self.out("\n");
        }

        for row in &rows[1..] {
            self.data_row(row, &widths);
        }

        // Bottom border
        self.bottom_border(&widths);
        self.out("\n");
    }

    fn top_border(&mut self, widths: &[usize]) {
        self.out("┌");
        for (i, w) in widths.iter().enumerate() {
            self.out(&"─".repeat(w + 2));
            if i < widths.len() - 1 {
                self.out("┬");
            }
        }
        self.out("┐");
    }

    fn sep_border(&mut self, widths: &[usize]) {
        self.out("├");
        for (i, w) in widths.iter().enumerate() {
            self.out(&"─".repeat(w + 2));
            if i < widths.len() - 1 {
                self.out("┼");
            }
        }
        self.out("┤\n");
    }

    fn bottom_border(&mut self, widths: &[usize]) {
        self.out("└");
        for (i, w) in widths.iter().enumerate() {
            self.out(&"─".repeat(w + 2));
            if i < widths.len() - 1 {
                self.out("┴");
            }
        }
        self.out("┘\n");
    }

    fn data_row(&mut self, row: &[String], widths: &[usize]) {
        self.out("│");
        for (i, cell) in row.iter().enumerate() {
            let w = widths.get(i).copied().unwrap_or(3);
            self.out(" ");
            let visible_len = visible_width(cell);
            self.out(cell);
            if visible_len < w {
                self.out(&" ".repeat(w - visible_len));
            }
            self.out(" │");
        }
        self.out("\n");
    }
}

/// Apply data-specific ANSI styling to plain inline text (cost, tokens, diffs, finished markers).
/// Only called for `Event::Text` — never applied inside code blocks or inline code.
fn style_data_text(text: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;

    const GREEN: &str = "\x1b[32m";
    const RED: &str = "\x1b[31m";
    const YELLOW: &str = "\x1b[33m";
    const MAGENTA: &str = "\x1b[35m";
    const BOLD: &str = "\x1b[1m";
    const RESET: &str = "\x1b[0m";

    fn diff_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\((\+)(\d+)/-(\d+)\)").unwrap())
    }
    fn cost_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\$[0-9]+\.[0-9]+").unwrap())
    }
    fn tok_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"\b[0-9]+ tokens\b").unwrap())
    }
    fn fin_re() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"(?m)^> ⏹ .+$").unwrap())
    }

    let s = fin_re().replace_all(text, |caps: &regex::Captures| {
        format!("{BOLD}{YELLOW}{}{RESET}", &caps[0])
    });
    let s = diff_re().replace_all(&s, |caps: &regex::Captures| {
        format!("({GREEN}+{}{RESET}/{RED}-{}{RESET})", &caps[2], &caps[3])
    });
    let s = cost_re().replace_all(&s, |caps: &regex::Captures| {
        format!("{YELLOW}{}{RESET}", &caps[0])
    });
    let s = tok_re().replace_all(&s, |caps: &regex::Captures| {
        format!("{MAGENTA}{}{RESET}", &caps[0])
    });
    s.into_owned()
}
