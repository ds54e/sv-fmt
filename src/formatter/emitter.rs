use crate::config::FormatConfig;

pub(crate) struct Emitter<'a> {
    config: &'a FormatConfig,
    output: String,
    indent_level: usize,
    at_line_start: bool,
    pending_space: bool,
    last_line_was_comment: bool,
}

impl<'a> Emitter<'a> {
    pub(crate) fn new(config: &'a FormatConfig) -> Self {
        Self {
            config,
            output: String::new(),
            indent_level: 0,
            at_line_start: true,
            pending_space: false,
            last_line_was_comment: false,
        }
    }

    pub(crate) fn buffer(&self) -> &str {
        &self.output
    }

    pub(crate) fn indent_level(&self) -> usize {
        self.indent_level
    }

    pub(crate) fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    pub(crate) fn decrease_indent(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    pub(crate) fn at_line_start(&self) -> bool {
        self.at_line_start
    }

    pub(crate) fn set_at_line_start(&mut self, value: bool) {
        self.at_line_start = value;
    }

    pub(crate) fn pending_space(&self) -> bool {
        self.pending_space
    }

    pub(crate) fn set_pending_space(&mut self, value: bool) {
        self.pending_space = value;
    }

    pub(crate) fn last_line_was_comment(&self) -> bool {
        self.last_line_was_comment
    }

    pub(crate) fn set_last_line_was_comment(&mut self, value: bool) {
        self.last_line_was_comment = value;
    }

    pub(crate) fn write_indent(&mut self) {
        if self.config.use_tabs {
            for _ in 0..self.indent_level {
                self.output.push('\t');
            }
        } else {
            self.output
                .push_str(&" ".repeat(self.indent_level * self.config.indent_width));
        }
        self.at_line_start = false;
        self.pending_space = false;
    }

    pub(crate) fn push_str(&mut self, text: &str) {
        self.output.push_str(text);
    }

    pub(crate) fn push_char(&mut self, ch: char) {
        self.output.push(ch);
    }

    pub(crate) fn ensure_trailing_newline(&mut self) {
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    pub(crate) fn take_output(&mut self) -> String {
        std::mem::take(&mut self.output)
    }

    pub(crate) fn newline(&mut self) {
        self.trim_trailing_whitespace();
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.at_line_start = true;
        self.pending_space = false;
    }

    pub(crate) fn ensure_blank_line(&mut self) {
        self.trim_trailing_whitespace();
        if self.output.is_empty() {
            self.at_line_start = true;
            return;
        }
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        if !self.output.ends_with("\n\n") {
            self.output.push('\n');
        }
        self.at_line_start = true;
        self.pending_space = false;
    }

    pub(crate) fn ensure_blank_line_after_comment(&mut self) {
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        if !self.output.ends_with("\n\n") {
            self.output.push('\n');
        }
        self.at_line_start = true;
        self.pending_space = false;
    }

    pub(crate) fn trim_trailing_whitespace(&mut self) {
        while self.output.ends_with(' ') || self.output.ends_with('\t') {
            self.output.pop();
        }
    }
}
