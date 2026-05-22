use crossterm::event::{KeyCode, KeyModifiers};

/// Trait for loading more lines into the pager on demand.
/// Called when the user scrolls to the top of the currently loaded content.
pub trait Loader {
    fn load_more(&mut self) -> Vec<String>;
}

pub struct Pager {
    lines: Vec<String>,
    scroll_pos: usize,
    search_mode: bool,
    search_query: String,
    search_matches: Vec<usize>,
    search_current: usize,
    goto_mode: bool,
    goto_buf: String,
    status_prefix: String,
    loader: Option<Box<dyn Loader>>,
    loader_header: usize,
}

const SCROLL_LINES: usize = 3;
const LOAD_THRESHOLD: usize = 5;

impl Pager {
    pub fn new(lines: Vec<String>) -> Self {
        Pager {
            lines,
            scroll_pos: 0,
            search_mode: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_current: 0,
            goto_mode: false,
            goto_buf: String::new(),
            status_prefix: String::new(),
            loader: None,
            loader_header: 0,
        }
    }

    pub fn with_status_prefix(mut self, prefix: &str) -> Self {
        self.status_prefix = prefix.to_string();
        self
    }

    /// Register a loader that provides additional lines when the user scrolls
    /// near the top of the currently loaded content.
    pub fn set_loader(&mut self, header_count: usize, loader: Box<dyn Loader>) {
        self.loader = Some(loader);
        self.loader_header = header_count;
    }

    /// Run the full-screen pager event loop.
    /// Starts at the bottom of content, returns Ok(()) when user presses q or Esc.
    pub fn run(&mut self) -> anyhow::Result<()> {
        use crossterm::cursor;
        use crossterm::event::{self, Event, MouseEventKind};
        use crossterm::execute;
        use crossterm::terminal::{self, Clear, ClearType};
        use std::io::Write as _;

        if self.lines.is_empty() {
            return Ok(());
        }

        terminal::enable_raw_mode()?;
        execute!(std::io::stdout(), event::EnableMouseCapture)?;
        struct PagerGuard;
        impl Drop for PagerGuard {
            fn drop(&mut self) {
                let _ = terminal::disable_raw_mode();
                let _ = execute!(std::io::stdout(), event::DisableMouseCapture);
            }
        }
        let _guard = PagerGuard;
        let mut stdout = std::io::stdout();

        // Start at bottom
        if let Ok((_, term_height)) = terminal::size() {
            let visible_h = term_height.saturating_sub(1) as usize;
            if self.lines.len() > visible_h {
                self.scroll_pos = self.lines.len() - visible_h;
            }
        }

        let result: anyhow::Result<()> = loop {
            // Try to load more content when scrolling near the top
            if self.scroll_pos <= LOAD_THRESHOLD {
                self.try_load_more();
            }

            let (term_width, term_height) = terminal::size()?;
            let visible_h = term_height.saturating_sub(1) as usize;

            // Clamp scroll position
            if self.lines.len() > visible_h && self.scroll_pos + visible_h > self.lines.len() {
                self.scroll_pos = self.lines.len() - visible_h;
            } else if self.lines.len() <= visible_h {
                self.scroll_pos = 0;
            }

            let end = std::cmp::min(self.scroll_pos + visible_h, self.lines.len());

            execute!(stdout, cursor::MoveTo(0, 0), Clear(ClearType::All))?;
            let match_set: std::collections::HashSet<usize> = self.search_matches.iter().copied().collect();
            for (rel_idx, line) in self.lines[self.scroll_pos..end].iter().enumerate() {
                let abs_idx = self.scroll_pos + rel_idx;
                let display = truncate_ansi(line, term_width as usize);
                if !self.search_matches.is_empty()
                    && self.search_current < self.search_matches.len()
                    && abs_idx == self.search_matches[self.search_current]
                {
                    write!(stdout, "\x1b[7m{}\x1b[0m\r\n", display)?;
                } else if match_set.contains(&abs_idx) {
                    write!(stdout, "\x1b[48;5;236m{}\x1b[0m\r\n", display)?;
                } else {
                    write!(stdout, "{}\r\n", display)?;
                }
            }

            let total = self.lines.len();
            let pct = if total <= 1 { 100 } else { ((end as f64 / total as f64) * 100.0) as usize };
            let status = if self.goto_mode {
                format!("\x1b[7m :{} \x1b[0m", self.goto_buf)
            } else if self.search_mode {
                format!("\x1b[7m /{} \x1b[0m", self.search_query)
            } else if !self.search_matches.is_empty() {
                format!(
                    "\x1b[7m {}L{}-{}/{} ({}%) | match {}/{} | n/N \x1b[0m",
                    self.status_prefix,
                    self.scroll_pos + 1,
                    end,
                    total,
                    pct,
                    self.search_current + 1,
                    self.search_matches.len(),
                )
            } else {
                format!(
                    "\x1b[7m {}L{}-{}/{} ({}%) | ↑↓ PgUp PgDn Ctrl+U/D g G / q \x1b[0m",
                    self.status_prefix, self.scroll_pos + 1, end, total, pct,
                )
            };
            let truncated: String = status.chars().take(term_width as usize).collect();
            write!(stdout, "\x1b[{};1H{}", term_height, truncated)?;
            stdout.flush()?;

            match event::read() {
                Ok(Event::Key(key)) => {
                    if self.goto_mode {
                        self.handle_goto_key(key);
                    } else if self.search_mode {
                        self.handle_search_key(key);
                    } else {
                        if self.handle_normal_key(key, visible_h)? {
                            break Ok(());
                        }
                    }
                }
                Ok(Event::Resize(_, _)) => {}
                Ok(Event::Mouse(me)) => match me.kind {
                    MouseEventKind::ScrollUp if self.scroll_pos > 0 => {
                        self.scroll_pos = self.scroll_pos.saturating_sub(SCROLL_LINES);
                    }
                    MouseEventKind::ScrollDown
                        if self.scroll_pos + visible_h < self.lines.len() =>
                    {
                        self.scroll_pos = std::cmp::min(
                            self.scroll_pos + SCROLL_LINES,
                            self.lines.len().saturating_sub(visible_h),
                        );
                    }
                    _ => {}
                },
                Ok(_) => {}
                Err(e) => anyhow::bail!("Pager input error: {e}"),
            }
        };

        execute!(stdout, cursor::MoveTo(0, 0), Clear(ClearType::All))?;
        stdout.flush()?;
        result
    }

    fn try_load_more(&mut self) {
        let hc = self.loader_header;
        let new_lines = {
            let loader = match self.loader.as_mut() {
                Some(l) => l,
                None => return,
            };
            loader.load_more()
        };
        if new_lines.is_empty() {
            self.loader = None;
            return;
        }
        let n = new_lines.len();
        self.lines.splice(hc..hc, new_lines);
        if self.scroll_pos > hc {
            self.scroll_pos += n;
        }
    }

    fn handle_search_key(&mut self, key: crossterm::event::KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Char(c), _) if !c.is_control() => {
                self.search_query.push(c);
            }
            (KeyCode::Backspace, _) => {
                self.search_query.pop();
            }
            (KeyCode::Enter, _) => {
                self.execute_search();
                self.search_mode = false;
            }
            (KeyCode::Esc, _) => {
                self.search_mode = false;
                self.search_query.clear();
            }
            _ => {}
        }
    }

    fn handle_goto_key(&mut self, key: crossterm::event::KeyEvent) {
        match (key.code, key.modifiers) {
            (KeyCode::Char(c), _) if c.is_ascii_digit() => {
                self.goto_buf.push(c);
            }
            (KeyCode::Backspace, _) => {
                self.goto_buf.pop();
            }
            (KeyCode::Enter, _) => {
                if let Ok(line_num) = self.goto_buf.parse::<usize>() {
                    if line_num > 0 {
                        let target = line_num.saturating_sub(1);
                        let max_pos = self.lines.len().saturating_sub(1);
                        self.scroll_pos = target.min(max_pos);
                    }
                }
                self.goto_mode = false;
                self.goto_buf.clear();
            }
            (KeyCode::Esc, _) => {
                self.goto_mode = false;
                self.goto_buf.clear();
            }
            _ => {}
        }
    }

    fn handle_normal_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        visible_h: usize,
    ) -> anyhow::Result<bool> {
        match (key.code, key.modifiers) {
            (KeyCode::Up, _) | (KeyCode::Char('k'), _) if self.scroll_pos > 0 => {
                self.scroll_pos -= 1;
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), _)
                if self.scroll_pos + visible_h < self.lines.len() =>
            {
                self.scroll_pos += 1;
            }
            (KeyCode::PageUp, _) => {
                self.scroll_pos = self.scroll_pos.saturating_sub(visible_h);
            }
            (KeyCode::PageDown, _) => {
                self.scroll_pos = std::cmp::min(
                    self.scroll_pos + visible_h,
                    self.lines.len().saturating_sub(visible_h),
                );
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                let half = (visible_h / 2).max(1);
                self.scroll_pos = self.scroll_pos.saturating_sub(half);
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                let half = (visible_h / 2).max(1);
                self.scroll_pos = std::cmp::min(
                    self.scroll_pos + half,
                    self.lines.len().saturating_sub(visible_h),
                );
            }
            (KeyCode::Home, _) | (KeyCode::Char('g'), _) => {
                self.scroll_pos = 0;
            }
            (KeyCode::End, _) | (KeyCode::Char('G'), _) => {
                self.scroll_pos = self.lines.len().saturating_sub(visible_h);
            }
            (KeyCode::Char(':'), _) => {
                self.goto_mode = true;
                self.goto_buf.clear();
            }
            (KeyCode::Char('/'), _) => {
                self.search_mode = true;
                self.search_query.clear();
            }
            (KeyCode::Char('n'), _) if !self.search_matches.is_empty() => {
                self.search_current = (self.search_current + 1) % self.search_matches.len();
                self.scroll_pos = self.search_matches[self.search_current];
            }
            (KeyCode::Char('N'), _) if !self.search_matches.is_empty() => {
                self.search_current = if self.search_current == 0 {
                    self.search_matches.len() - 1
                } else {
                    self.search_current - 1
                };
                self.scroll_pos = self.search_matches[self.search_current];
            }
            (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => return Ok(true),
            _ => {}
        }
        Ok(false)
    }

    /// Re-run the search across all lines (case-insensitive, ANSI-stripped).
    fn execute_search(&mut self) {
        self.search_matches.clear();
        self.search_current = 0;
        if !self.search_query.is_empty() {
            let lower = self.search_query.to_lowercase();
            for (i, line) in self.lines.iter().enumerate() {
                let plain = crate::render::strip_ansi(line);
                if plain.to_lowercase().contains(&lower) {
                    self.search_matches.push(i);
                }
            }
            if !self.search_matches.is_empty() {
                self.search_current = 0;
                self.scroll_pos = self.search_matches[0];
            }
        }
    }
}

/// Truncate a (potentially ANSI-escaped) string to `max_width` visible columns,
/// preserving ANSI escape sequences across the truncation boundary.
fn truncate_ansi(line: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mut visible = 0usize;
    let mut escaped = false;
    let mut result = String::with_capacity(line.len());
    for c in line.chars() {
        if escaped {
            result.push(c);
            if c == 'm' {
                escaped = false;
            }
            continue;
        }
        if c == '\x1b' {
            escaped = true;
            result.push(c);
            continue;
        }
        if visible >= max_width {
            continue;
        }
        visible += 1;
        result.push(c);
    }
    if escaped {
        result.push('m');
    }
    result
}
