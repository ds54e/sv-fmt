use std::collections::HashMap;

use anyhow::Result;

use super::{
    analysis::{ByteSpan, collect_case_alignment, collect_statement_spans},
    emitter::Emitter,
    lexer::{Token, TokenKind, tokenize},
    wrapping::wrap_formatted_output,
};
use crate::{
    config::FormatConfig,
    parser::{self, SvParserCfg},
};

pub fn format_text(input: &str, config: &FormatConfig) -> Result<String> {
    let tree = parser::parse(input, &SvParserCfg::default())?;
    let body_spans = collect_statement_spans(&tree);
    let case_alignment = collect_case_alignment(&tree);
    let tokens = tokenize(&tree);
    let mut formatter = Formatter::new(config, &tokens, body_spans, case_alignment);
    formatter.format()
}

struct Formatter<'a> {
    config: &'a FormatConfig,
    tokens: &'a [Token],
    body_spans: HashMap<usize, ByteSpan>,
    case_alignment: HashMap<usize, usize>,
    idx: usize,
    emitter: Emitter<'a>,
    previous_call_ident: bool,
    inserted_blocks: Vec<usize>,
    wrap_tracker: WrapTracker,
}

struct WrapTracker {
    mode: WrapMode,
    paren_depth: usize,
    keyword: Option<WrapKeyword>,
    body_span: Option<ByteSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WrapMode {
    Idle,
    WaitingCondition,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WrapKeyword {
    If,
    Else,
    For,
    Foreach,
    While,
    Do,
    Forever,
}

impl WrapKeyword {
    fn from_token(token: &Token) -> Option<Self> {
        if token.is_keyword("if") {
            Some(Self::If)
        } else if token.is_keyword("else") {
            Some(Self::Else)
        } else if token.is_keyword("for") {
            Some(Self::For)
        } else if token.is_keyword("foreach") {
            Some(Self::Foreach)
        } else if token.is_keyword("while") {
            Some(Self::While)
        } else if token.is_keyword("do") {
            Some(Self::Do)
        } else if token.is_keyword("forever") {
            Some(Self::Forever)
        } else {
            None
        }
    }
}

impl<'a> Formatter<'a> {
    fn new(
        config: &'a FormatConfig,
        tokens: &'a [Token],
        body_spans: HashMap<usize, ByteSpan>,
        case_alignment: HashMap<usize, usize>,
    ) -> Self {
        Self {
            config,
            tokens,
            body_spans,
            case_alignment,
            idx: 0,
            emitter: Emitter::new(config),
            previous_call_ident: false,
            inserted_blocks: Vec::new(),
            wrap_tracker: WrapTracker::new(),
        }
    }

    fn format(&mut self) -> Result<String> {
        while self.idx < self.tokens.len() {
            let token = &self.tokens[self.idx];
            match token.kind {
                TokenKind::Newline => self.handle_newline(),
                TokenKind::Comment => self.handle_comment(token),
                TokenKind::Directive => self.handle_directive(token),
                _ => self.handle_token(token),
            }
            self.idx += 1;
        }

        if self.config.wrap_multiline_blocks {
            while let Some(_) = self.inserted_blocks.pop() {
                self.insert_auto_end();
            }
        }

        self.emitter.ensure_trailing_newline();

        let mut final_output = self.emitter.take_output();
        if self.config.auto_wrap_long_lines && self.config.max_line_length > 0 {
            final_output = wrap_formatted_output(final_output, self.config);
        }

        Ok(final_output)
    }

    fn handle_newline(&mut self) {
        if self.config.inline_end_else && self.prev_non_newline().map(|t| t.is_keyword("end")).unwrap_or(false) {
            if let Some(next) = self.peek_non_newline() {
                if next.is_keyword("else") {
                    self.emitter.set_pending_space(true);
                    return;
                }
            }
        }

        self.emitter.newline();
        self.previous_call_ident = false;

        if self.config.wrap_multiline_blocks {
            self.wrap_tracker.newline();
            self.maybe_insert_auto_begin();
        }
    }

    fn handle_comment(&mut self, token: &Token) {
        let text = token.text.trim_end_matches('\n');
        if text.trim_start().starts_with("/*") {
            self.emit_block_comment(text);
            return;
        }
        self.emit_line_comment(text, token.text.contains('\n'));
    }

    fn emit_line_comment(&mut self, text: &str, had_newline: bool) {
        if self.emitter.at_line_start() {
            self.emitter.write_indent();
        } else {
            self.emitter.trim_trailing_whitespace();
            if self.emitter.buffer().ends_with('\n') {
                self.emitter.write_indent();
            } else {
                self.emitter.push_char(' ');
            }
        }
        self.emitter.push_str(text);
        if had_newline {
            self.emitter.push_char('\n');
            self.emitter.set_at_line_start(true);
        } else {
            self.emitter.set_at_line_start(false);
        }
        self.emitter.set_pending_space(false);
        self.previous_call_ident = false;
        self.emitter.set_last_line_was_comment(true);
    }

    fn emit_block_comment(&mut self, text: &str) {
        self.emitter.ensure_blank_line();
        self.emitter.write_indent();
        self.emitter.push_str(text);
        self.emitter.push_char('\n');
        self.emitter.set_at_line_start(true);
        self.emitter.set_pending_space(false);
        self.previous_call_ident = false;
        self.emitter.ensure_blank_line_after_comment();
        self.emitter.set_last_line_was_comment(true);
    }

    fn maybe_insert_section_spacing(&mut self, token: &Token) {
        if !is_section_decl_keyword(token) {
            return;
        }
        if self.emitter.buffer().is_empty() {
            return;
        }
        if self.emitter.last_line_was_comment() {
            return;
        }
        self.emitter.ensure_blank_line();
        self.emitter.set_last_line_was_comment(false);
    }

    fn handle_directive(&mut self, token: &Token) {
        if !self.emitter.at_line_start() {
            self.emitter.newline();
        }
        self.emitter.push_str(&token.text);
        self.emitter.set_at_line_start(false);
        self.emitter.set_pending_space(false);
        self.emitter.set_last_line_was_comment(false);
    }

    fn handle_token(&mut self, token: &Token) {
        if self.config.wrap_multiline_blocks {
            self.flush_auto_ends_before(token);
            self.wrap_tracker.observe_token(token);
        }

        if is_dedent_keyword(token) {
            self.emitter.decrease_indent();
        }

        if self.config.align_case_colon && token.text == ":" {
            if self.apply_case_alignment(token) {
                return;
            }
        }

        if self.emitter.at_line_start() {
            self.maybe_insert_section_spacing(token);
            self.emitter.write_indent();
        } else if self.emitter.pending_space() && !needs_no_space_before(&token.text) {
            self.emitter.push_char(' ');
        }

        if token.text == "," && self.config.space_after_comma {
            self.emitter.trim_trailing_whitespace();
            self.emitter.push_char(',');
            self.emitter.set_pending_space(true);
        } else if token.text == "(" && self.config.remove_call_space && self.previous_call_ident {
            self.emitter.trim_trailing_whitespace();
            self.emitter.push_char('(');
            self.emitter.set_pending_space(false);
        } else {
            self.emitter.push_str(&token.text);
            let needs_space = needs_space_after(&token.text, self.peek_non_newline());
            self.emitter.set_pending_space(needs_space);
        }

        if is_indent_keyword(token) {
            self.emitter.increase_indent();
        }

        self.emitter.set_at_line_start(false);
        self.previous_call_ident = token.is_identifier_like();
        self.emitter.set_last_line_was_comment(false);

        if self.config.wrap_multiline_blocks {
            let span = self.body_spans.get(&token.offset).cloned();
            self.wrap_tracker.maybe_start(token, span);
        }
    }

    fn apply_case_alignment(&mut self, token: &Token) -> bool {
        if let Some(padding) = self.case_alignment.get(&token.offset).copied() {
            self.emitter.trim_trailing_whitespace();
            for _ in 0..padding {
                self.emitter.push_char(' ');
            }
            self.emitter.push_char(':');
            self.emitter.set_pending_space(true);
            self.emitter.set_at_line_start(false);
            self.previous_call_ident = false;
            return true;
        }
        false
    }

    fn prev_non_newline(&self) -> Option<&Token> {
        if self.idx == 0 {
            return None;
        }
        self.tokens[..self.idx]
            .iter()
            .rev()
            .find(|tok| tok.kind != TokenKind::Newline)
    }

    fn peek_non_newline(&self) -> Option<&Token> {
        if self.idx + 1 >= self.tokens.len() {
            return None;
        }
        self.tokens[self.idx + 1..]
            .iter()
            .find(|tok| tok.kind != TokenKind::Newline)
    }

    fn maybe_insert_auto_begin(&mut self) {
        if !self.config.wrap_multiline_blocks {
            return;
        }
        if self.wrap_tracker.ready_to_wrap() {
            if self.wrap_tracker.body_needs_wrap(&self.tokens, self.idx + 1) {
                self.emitter.write_indent();
                self.emitter.push_str("begin");
                self.emitter.push_char('\n');
                self.emitter.increase_indent();
                self.emitter.set_at_line_start(true);
                self.emitter.set_pending_space(false);
                self.inserted_blocks.push(self.emitter.indent_level());
            }
            self.wrap_tracker.reset();
        }
    }

    fn flush_auto_ends_before(&mut self, next: &Token) {
        if !self.config.wrap_multiline_blocks {
            return;
        }
        if self.inserted_blocks.is_empty() {
            return;
        }
        if next.is_keyword("else") || is_dedent_keyword(next) {
            self.insert_auto_end();
            self.inserted_blocks.pop();
        }
    }

    fn insert_auto_end(&mut self) {
        self.emitter.trim_trailing_whitespace();
        self.emitter.ensure_trailing_newline();
        self.emitter.decrease_indent();
        self.emitter.set_at_line_start(true);
        self.emitter.set_pending_space(false);
        self.emitter.write_indent();
        self.emitter.push_str("end");
        self.emitter.push_char('\n');
        self.emitter.set_at_line_start(true);
        self.emitter.set_pending_space(false);
        self.previous_call_ident = false;
    }
}

fn needs_space_after(text: &str, next: Option<&Token>) -> bool {
    match text {
        "(" | "[" | "{" | "." | "@" => false,
        ")" | "]" | "}" | ";" | "," => true,
        ":" => matches!(next, Some(tok) if !tok.is_symbol(":")),
        _ => true,
    }
}

fn needs_no_space_before(text: &str) -> bool {
    matches!(text, ")" | "]" | "}" | "," | ";" | ".")
}

fn is_indent_keyword(token: &Token) -> bool {
    token.is_keyword("module")
        || token.is_keyword("class")
        || token.is_keyword("function")
        || token.is_keyword("task")
        || token.is_keyword("package")
        || token.is_keyword("begin")
        || token.is_keyword("case")
        || token.is_keyword("casex")
        || token.is_keyword("casez")
        || token.is_keyword("randcase")
        || token.is_keyword("randsequence")
        || token.is_keyword("covergroup")
        || token.is_keyword("fork")
        || token.is_keyword("generate")
        || token.is_keyword("interface")
}

fn is_section_decl_keyword(token: &Token) -> bool {
    token.is_keyword("package") || token.is_keyword("class") || token.is_keyword("interface")
}

fn is_dedent_keyword(token: &Token) -> bool {
    token.is_keyword("end")
        || token.is_keyword("endmodule")
        || token.is_keyword("endclass")
        || token.is_keyword("endfunction")
        || token.is_keyword("endtask")
        || token.is_keyword("endcase")
        || token.is_keyword("endsequence")
        || token.is_keyword("endpackage")
        || token.is_keyword("endgroup")
        || token.is_keyword("endgenerate")
        || token.is_keyword("join")
        || token.is_keyword("join_any")
        || token.is_keyword("join_none")
}

impl WrapTracker {
    fn new() -> Self {
        Self {
            mode: WrapMode::Idle,
            paren_depth: 0,
            keyword: None,
            body_span: None,
        }
    }

    fn reset(&mut self) {
        self.mode = WrapMode::Idle;
        self.paren_depth = 0;
        self.keyword = None;
        self.body_span = None;
    }

    fn newline(&mut self) {}

    fn maybe_start(&mut self, token: &Token, span: Option<ByteSpan>) {
        let kw = WrapKeyword::from_token(token);
        if let Some(kw) = kw {
            self.body_span = span;
            self.mode = match kw {
                WrapKeyword::If | WrapKeyword::For | WrapKeyword::Foreach | WrapKeyword::While => {
                    WrapMode::WaitingCondition
                }
                WrapKeyword::Else | WrapKeyword::Do | WrapKeyword::Forever => WrapMode::Ready,
            };
            self.paren_depth = 0;
            self.keyword = Some(kw);
        }
    }

    fn observe_token(&mut self, token: &Token) {
        match self.mode {
            WrapMode::Idle => {}
            WrapMode::WaitingCondition => match token.text.as_str() {
                "(" => self.paren_depth += 1,
                ")" => {
                    if self.paren_depth > 0 {
                        self.paren_depth -= 1;
                    }
                    if self.paren_depth == 0 {
                        self.mode = WrapMode::Ready;
                    }
                }
                _ => {}
            },
            WrapMode::Ready => {
                if token.is_keyword("begin") || token.text == ";" || is_dedent_keyword(token) {
                    self.reset();
                }
            }
        }
    }

    fn ready_to_wrap(&self) -> bool {
        matches!(self.mode, WrapMode::Ready)
    }

    fn body_needs_wrap(&self, tokens: &[Token], index: usize) -> bool {
        let keyword = match self.keyword {
            Some(k) => k,
            None => return false,
        };

        let mut semicolons = 0usize;
        let mut inspected = 0usize;
        let span_end = self.body_span.map(|span| span.end);
        let required = if self.body_span.is_some() { 1 } else { 2 };
        for token in tokens.iter().skip(index) {
            if matches!(token.kind, TokenKind::Newline) {
                continue;
            }
            if let Some(end) = span_end {
                if token.offset < end {
                    continue;
                }
            }
            if token.is_keyword("begin") {
                return false;
            }
            if matches!(keyword, WrapKeyword::Else) && token.is_keyword("if") {
                return false;
            }
            if token.is_keyword("else") || is_dedent_keyword(token) {
                break;
            }
            if token.text == ";" {
                semicolons += 1;
                if semicolons >= required {
                    break;
                }
            }
            inspected += 1;
            if inspected >= 128 {
                break;
            }
        }

        semicolons >= required
    }
}
