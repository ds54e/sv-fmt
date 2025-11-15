use std::collections::{HashMap, HashSet};

use anyhow::Result;
use once_cell::sync::Lazy;
use sv_parser::{Iter, NodeEvent, RefNode, RefNodes, SyntaxTree};

use crate::{
    config::FormatConfig,
    parser::{self, SvParserCfg},
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Keyword,
    Identifier,
    Symbol,
    Number,
    StringLiteral,
    Comment,
    Directive,
    Newline,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    kind: TokenKind,
    offset: usize,
    len: usize,
}

impl Token {
    fn new_spanned(text: impl Into<String>, kind: TokenKind, offset: usize, len: usize) -> Self {
        Self {
            text: text.into(),
            kind,
            offset,
            len,
        }
    }

    fn is_keyword(&self, needle: &str) -> bool {
        matches!(self.kind, TokenKind::Keyword) && self.text.eq_ignore_ascii_case(needle)
    }

    fn is_identifier_like(&self) -> bool {
        matches!(self.kind, TokenKind::Identifier)
    }

    fn is_symbol(&self, needle: &str) -> bool {
        matches!(self.kind, TokenKind::Symbol) && self.text == needle
    }

    fn lowered(&self) -> String {
        self.text.to_ascii_lowercase()
    }
}

pub fn format_text(input: &str, config: &FormatConfig) -> Result<String> {
    let tree = parser::parse(input, &SvParserCfg::default())?;
    let body_spans = collect_statement_spans(&tree);
    let case_alignment = collect_case_alignment(&tree);
    let tokens = tokenize(&tree);
    let mut formatter = Formatter::new(config, tokens, body_spans, case_alignment);
    formatter.format()
}

fn tokenize(tree: &SyntaxTree) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut whitespace_depth = 0usize;
    let mut comment_depth = 0usize;
    let mut directive_depth = 0usize;

    for event in tree.into_iter().event() {
        match event {
            NodeEvent::Enter(node) => match node {
                RefNode::WhiteSpace(_) => whitespace_depth += 1,
                RefNode::Comment(_) => comment_depth += 1,
                RefNode::CompilerDirective(_) => directive_depth += 1,
                RefNode::Locate(loc) => {
                    if let Some(text) = tree.get_str(loc) {
                        handle_locate(
                            text,
                            loc.offset,
                            whitespace_depth,
                            comment_depth,
                            directive_depth,
                            &mut tokens,
                        );
                    }
                }
                _ => {}
            },
            NodeEvent::Leave(node) => match node {
                RefNode::WhiteSpace(_) => whitespace_depth = whitespace_depth.saturating_sub(1),
                RefNode::Comment(_) => comment_depth = comment_depth.saturating_sub(1),
                RefNode::CompilerDirective(_) => directive_depth = directive_depth.saturating_sub(1),
                _ => {}
            },
        }
    }

    tokens
}

fn handle_locate(
    text: &str,
    offset: usize,
    whitespace_depth: usize,
    comment_depth: usize,
    directive_depth: usize,
    tokens: &mut Vec<Token>,
) {
    if text.is_empty() {
        return;
    }
    if comment_depth > 0 {
        tokens.push(Token::new_spanned(text, TokenKind::Comment, offset, text.len()));
        return;
    }
    if whitespace_depth > 0 {
        let mut current_offset = offset;
        for ch in text.chars() {
            if ch == '\n' {
                tokens.push(Token::new_spanned("\n", TokenKind::Newline, current_offset, 1));
            }
            current_offset += ch.len_utf8();
        }
        return;
    }
    if directive_depth > 0 {
        tokens.push(Token::new_spanned(text, TokenKind::Directive, offset, text.len()));
        return;
    }

    tokens.push(Token::new_spanned(text, classify_token(text), offset, text.len()));
}

#[derive(Debug, Clone, Copy)]
struct ByteSpan {
    start: usize,
    end: usize,
}

impl ByteSpan {
    fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }
}

fn collect_statement_spans(tree: &SyntaxTree) -> HashMap<usize, ByteSpan> {
    let mut spans = HashMap::new();
    for event in tree.into_iter().event() {
        match event {
            NodeEvent::Enter(RefNode::ConditionalStatement(stmt)) => {
                let (_, if_kw, _, then_stmt, else_ifs, else_stmt) = &stmt.nodes;
                record_span(if_kw, RefNode::StatementOrNull(then_stmt), &mut spans);
                for (_, elseif_kw, _, body) in else_ifs {
                    record_span(elseif_kw, RefNode::StatementOrNull(body), &mut spans);
                }
                if let Some((else_kw, body)) = else_stmt {
                    record_span(else_kw, RefNode::StatementOrNull(body), &mut spans);
                }
            }
            NodeEvent::Enter(RefNode::LoopStatement(stmt)) => match stmt {
                sv_parser::LoopStatement::Forever(node) => {
                    record_span(&node.nodes.0, RefNode::StatementOrNull(&node.nodes.1), &mut spans)
                }
                sv_parser::LoopStatement::Repeat(node) => {
                    record_span(&node.nodes.0, RefNode::StatementOrNull(&node.nodes.2), &mut spans)
                }
                sv_parser::LoopStatement::While(node) => {
                    record_span(&node.nodes.0, RefNode::StatementOrNull(&node.nodes.2), &mut spans)
                }
                sv_parser::LoopStatement::For(node) => {
                    record_span(&node.nodes.0, RefNode::StatementOrNull(&node.nodes.2), &mut spans)
                }
                sv_parser::LoopStatement::DoWhile(node) => {
                    record_span(&node.nodes.0, RefNode::StatementOrNull(&node.nodes.1), &mut spans)
                }
                sv_parser::LoopStatement::Foreach(node) => {
                    record_span(&node.nodes.0, RefNode::Statement(&node.nodes.2), &mut spans)
                }
            },
            _ => {}
        }
    }
    spans
}

fn record_span<'a>(keyword: &'a sv_parser::Keyword, node: RefNode<'a>, spans: &mut HashMap<usize, ByteSpan>) {
    if let Some(span) = node_span(node) {
        spans.insert(keyword.nodes.0.offset, span);
    }
}

fn node_span(node: RefNode) -> Option<ByteSpan> {
    let mut start = None;
    let mut end = 0;
    for event in node.into_iter().event() {
        if let NodeEvent::Enter(RefNode::Locate(loc)) = event {
            if start.is_none() {
                start = Some(loc.offset);
            }
            end = loc.offset + loc.len;
        }
    }
    start.map(|s| ByteSpan { start: s, end })
}

fn collect_case_alignment(tree: &SyntaxTree) -> HashMap<usize, usize> {
    let mut alignment = HashMap::new();
    for event in tree.into_iter().event() {
        if let NodeEvent::Enter(RefNode::CaseStatement(stmt)) = event {
            if let sv_parser::CaseStatement::Normal(case) = stmt {
                let mut entries = Vec::new();
                collect_case_item(&case.nodes.3, &mut entries);
                for item in &case.nodes.4 {
                    collect_case_item(item, &mut entries);
                }
                apply_alignment(entries, &mut alignment);
            }
        } else if let NodeEvent::Enter(RefNode::RandcaseStatement(stmt)) = event {
            let mut entries = Vec::new();
            collect_randcase_item(&stmt.nodes.1, &mut entries);
            for item in &stmt.nodes.2 {
                collect_randcase_item(item, &mut entries);
            }
            apply_alignment(entries, &mut alignment);
        }
    }
    alignment
}

fn collect_case_item(item: &sv_parser::CaseItem, entries: &mut Vec<(usize, usize)>) {
    match item {
        sv_parser::CaseItem::NonDefault(node) => {
            if let Some(start) = first_token_offset((&node.nodes.0).into()) {
                let symbol = &node.nodes.1.nodes.0;
                let width = symbol.offset.saturating_sub(start);
                entries.push((symbol.offset, width));
            }
        }
        sv_parser::CaseItem::Default(node) => {
            if let Some(symbol) = &node.nodes.1 {
                let start = node.nodes.0.nodes.0.offset;
                let width = symbol.nodes.0.offset.saturating_sub(start);
                entries.push((symbol.nodes.0.offset, width));
            }
        }
    }
}

fn collect_randcase_item(item: &sv_parser::RandcaseItem, entries: &mut Vec<(usize, usize)>) {
    if let Some(start) = first_token_offset((&item.nodes.0).into()) {
        let symbol = &item.nodes.1.nodes.0;
        let width = symbol.offset.saturating_sub(start);
        entries.push((symbol.offset, width));
    }
}

fn apply_alignment(entries: Vec<(usize, usize)>, alignment: &mut HashMap<usize, usize>) {
    if entries.len() < 2 {
        return;
    }
    if let Some(max_width) = entries.iter().map(|(_, width)| *width).max() {
        for (offset, width) in entries {
            let padding = max_width.saturating_sub(width) + 1;
            alignment.insert(offset, padding);
        }
    }
}

fn first_token_offset(nodes: RefNodes) -> Option<usize> {
    for event in Iter::new(nodes).event() {
        if let NodeEvent::Enter(RefNode::Locate(loc)) = event {
            return Some(loc.offset);
        }
    }
    None
}

fn classify_token(text: &str) -> TokenKind {
    let lowered = text.to_ascii_lowercase();
    if KEYWORDS.contains(lowered.as_str()) {
        TokenKind::Keyword
    } else if is_identifier(text) {
        TokenKind::Identifier
    } else if is_numeric_literal(text) {
        TokenKind::Number
    } else if is_string_literal(text) {
        TokenKind::StringLiteral
    } else if text.len() == 1 && is_symbol_char(text.chars().next().unwrap()) {
        TokenKind::Symbol
    } else {
        TokenKind::Other
    }
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(ch) if ch == '_' || ch == '$' || ch.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c == '$' || c.is_ascii_alphanumeric())
}

fn is_numeric_literal(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_digit() => {}
        _ => return false,
    }
    chars.all(|c| {
        c.is_ascii_digit()
            || matches!(
                c,
                '\'' | '_' | 'h' | 'H' | 'b' | 'B' | 'o' | 'O' | 'd' | 'D' | 'x' | 'X' | 'z' | 'Z'
            )
    })
}

fn is_string_literal(text: &str) -> bool {
    text.starts_with('"') && text.ends_with('"') && text.len() >= 2
}

fn is_symbol_char(ch: char) -> bool {
    matches!(
        ch,
        '(' | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | ','
            | ';'
            | ':'
            | '.'
            | '+'
            | '-'
            | '*'
            | '/'
            | '%'
            | '!'
            | '~'
            | '&'
            | '|'
            | '^'
            | '='
            | '<'
            | '>'
            | '?'
            | '@'
    )
}

struct Formatter<'a> {
    config: &'a FormatConfig,
    tokens: Vec<Token>,
    body_spans: HashMap<usize, ByteSpan>,
    case_alignment: HashMap<usize, usize>,
    idx: usize,
    output: String,
    indent_level: usize,
    at_line_start: bool,
    pending_space: bool,
    previous_call_ident: bool,
    inserted_blocks: Vec<usize>,
    wrap_tracker: WrapTracker,
    last_line_was_comment: bool,
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

impl<'a> Formatter<'a> {
    fn new(
        config: &'a FormatConfig,
        tokens: Vec<Token>,
        body_spans: HashMap<usize, ByteSpan>,
        case_alignment: HashMap<usize, usize>,
    ) -> Self {
        Self {
            config,
            tokens,
            body_spans,
            case_alignment,
            idx: 0,
            output: String::new(),
            indent_level: 0,
            at_line_start: true,
            pending_space: false,
            previous_call_ident: false,
            inserted_blocks: Vec::new(),
            wrap_tracker: WrapTracker::new(),
            last_line_was_comment: false,
        }
    }

    fn format(&mut self) -> Result<String> {
        while self.idx < self.tokens.len() {
            let token = self.tokens[self.idx].clone();
            match token.kind {
                TokenKind::Newline => self.handle_newline(),
                TokenKind::Comment => self.handle_comment(&token),
                TokenKind::Directive => self.handle_directive(&token),
                _ => self.handle_token(&token),
            }
            self.idx += 1;
        }

        if self.config.wrap_multiline_blocks {
            while let Some(_) = self.inserted_blocks.pop() {
                self.insert_auto_end();
            }
        }

        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }

        Ok(self.output.clone())
    }

    fn handle_newline(&mut self) {
        if self.config.inline_end_else && self.prev_non_newline().map(|t| t.is_keyword("end")).unwrap_or(false) {
            if let Some(next) = self.peek_non_newline() {
                if next.is_keyword("else") {
                    self.pending_space = true;
                    return;
                }
            }
        }

        self.trim_trailing_whitespace();
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.at_line_start = true;
        self.pending_space = false;
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
        if self.at_line_start {
            self.write_indent();
        } else {
            self.trim_trailing_whitespace();
            if self.output.ends_with('\n') {
                self.write_indent();
            } else {
                self.output.push(' ');
            }
        }
        self.output.push_str(text);
        if had_newline {
            self.output.push('\n');
            self.at_line_start = true;
        } else {
            self.at_line_start = false;
        }
        self.pending_space = false;
        self.previous_call_ident = false;
        self.last_line_was_comment = true;
    }

    fn emit_block_comment(&mut self, text: &str) {
        self.ensure_blank_line_before_block_comment();
        self.write_indent();
        self.output.push_str(text);
        self.output.push('\n');
        self.at_line_start = true;
        self.pending_space = false;
        self.previous_call_ident = false;
        self.ensure_blank_line_after_block_comment();
        self.last_line_was_comment = true;
    }

    fn ensure_blank_line_before_block_comment(&mut self) {
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
    }

    fn ensure_blank_line_after_block_comment(&mut self) {
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        if !self.output.ends_with("\n\n") {
            self.output.push('\n');
        }
        self.at_line_start = true;
    }

    fn maybe_insert_section_spacing(&mut self, keyword: &str) {
        if !is_section_decl_keyword(keyword) {
            return;
        }
        if self.output.is_empty() {
            return;
        }
        if self.last_line_was_comment {
            return;
        }
        self.trim_trailing_whitespace();
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        if !self.output.ends_with("\n\n") {
            self.output.push('\n');
        }
        self.at_line_start = true;
        self.pending_space = false;
        self.last_line_was_comment = false;
    }

    fn handle_directive(&mut self, token: &Token) {
        if !self.at_line_start {
            self.trim_trailing_whitespace();
            self.output.push('\n');
            self.at_line_start = true;
        }
        self.output.push_str(&token.text);
        self.at_line_start = false;
        self.pending_space = false;
        self.last_line_was_comment = false;
    }

    fn handle_token(&mut self, token: &Token) {
        if self.config.wrap_multiline_blocks {
            self.flush_auto_ends_before(token);
            self.wrap_tracker.observe_token(token);
        }

        let lowered = token.lowered();
        if is_dedent_keyword(&lowered) {
            self.indent_level = self.indent_level.saturating_sub(1);
        }

        if self.config.align_case_colon && token.text == ":" {
            if self.apply_case_alignment(token) {
                return;
            }
        }

        if self.at_line_start {
            self.maybe_insert_section_spacing(&lowered);
            self.write_indent();
        } else if self.pending_space && !needs_no_space_before(&token.text) {
            self.output.push(' ');
        }

        if token.text == "," && self.config.space_after_comma {
            self.strip_trailing_space();
            self.output.push(',');
            self.pending_space = true;
        } else if token.text == "(" && self.config.remove_call_space && self.previous_call_ident {
            self.strip_trailing_space();
            self.output.push('(');
            self.pending_space = false;
        } else {
            self.output.push_str(&token.text);
            self.pending_space = needs_space_after(&token.text, self.peek_non_newline());
        }

        if is_indent_keyword(&lowered) {
            self.indent_level += 1;
        }

        self.at_line_start = false;
        self.previous_call_ident = token.is_identifier_like();
        self.last_line_was_comment = false;

        if self.config.wrap_multiline_blocks {
            let span = self.body_spans.get(&token.offset).cloned();
            self.wrap_tracker.maybe_start(&lowered, span);
        }
    }

    fn apply_case_alignment(&mut self, token: &Token) -> bool {
        if let Some(padding) = self.case_alignment.get(&token.offset).copied() {
            self.trim_trailing_whitespace();
            for _ in 0..padding {
                self.output.push(' ');
            }
            self.output.push(':');
            self.pending_space = true;
            self.at_line_start = false;
            self.previous_call_ident = false;
            return true;
        }
        false
    }

    fn write_indent(&mut self) {
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

    fn trim_trailing_whitespace(&mut self) {
        while self.output.ends_with(' ') || self.output.ends_with('\t') {
            self.output.pop();
        }
    }

    fn strip_trailing_space(&mut self) {
        self.trim_trailing_whitespace();
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
                self.write_indent();
                self.output.push_str("begin");
                self.output.push('\n');
                self.indent_level += 1;
                self.at_line_start = true;
                self.pending_space = false;
                self.inserted_blocks.push(self.indent_level);
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
        if next.is_keyword("else") || is_dedent_keyword(&next.lowered()) {
            self.insert_auto_end();
            self.inserted_blocks.pop();
        }
    }

    fn insert_auto_end(&mut self) {
        self.trim_trailing_whitespace();
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.indent_level = self.indent_level.saturating_sub(1);
        self.at_line_start = true;
        self.pending_space = false;
        self.write_indent();
        self.output.push_str("end");
        self.output.push('\n');
        self.at_line_start = true;
        self.pending_space = false;
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

fn is_indent_keyword(keyword: &str) -> bool {
    matches!(
        keyword,
        "module"
            | "class"
            | "function"
            | "task"
            | "package"
            | "begin"
            | "case"
            | "casex"
            | "casez"
            | "randcase"
            | "randsequence"
            | "covergroup"
            | "fork"
            | "generate"
            | "interface"
    )
}

fn is_section_decl_keyword(keyword: &str) -> bool {
    matches!(keyword, "package" | "class" | "interface")
}

fn is_dedent_keyword(keyword: &str) -> bool {
    matches!(
        keyword,
        "end"
            | "endmodule"
            | "endclass"
            | "endfunction"
            | "endtask"
            | "endcase"
            | "endsequence"
            | "endpackage"
            | "endgroup"
            | "endgenerate"
            | "join"
            | "join_any"
            | "join_none"
    )
}

static KEYWORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "module",
        "endmodule",
        "class",
        "endclass",
        "function",
        "endfunction",
        "task",
        "endtask",
        "package",
        "endpackage",
        "begin",
        "end",
        "case",
        "endcase",
        "casex",
        "casez",
        "randcase",
        "randsequence",
        "endsequence",
        "fork",
        "join",
        "join_any",
        "join_none",
        "generate",
        "endgenerate",
        "interface",
        "endinterface",
        "covergroup",
        "endgroup",
        "if",
        "else",
        "for",
        "foreach",
        "while",
        "do",
        "forever",
    ]
    .into_iter()
    .collect()
});

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

    fn maybe_start(&mut self, keyword: &str, span: Option<ByteSpan>) {
        let kw = match keyword {
            "if" => Some(WrapKeyword::If),
            "else" => Some(WrapKeyword::Else),
            "for" => Some(WrapKeyword::For),
            "foreach" => Some(WrapKeyword::Foreach),
            "while" => Some(WrapKeyword::While),
            "do" => Some(WrapKeyword::Do),
            "forever" => Some(WrapKeyword::Forever),
            _ => None,
        };
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
                if token.is_keyword("begin") || token.text == ";" || is_dedent_keyword(&token.lowered()) {
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

        let threshold = if self.body_span.is_some() { 1 } else { 2 };
        let mut semicolons = 0usize;
        for token in tokens.iter().skip(index) {
            if matches!(token.kind, TokenKind::Newline) {
                continue;
            }
            if let Some(span) = &self.body_span {
                if span.contains(token.offset) {
                    continue;
                }
            }
            if token.is_keyword("begin") {
                return false;
            }
            if matches!(keyword, WrapKeyword::Else) && token.is_keyword("if") {
                return false;
            }
            if token.is_keyword("else") || is_dedent_keyword(&token.lowered()) {
                break;
            }
            if token.text == ";" {
                semicolons += 1;
                if semicolons >= threshold {
                    break;
                }
            }
        }

        semicolons >= threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> FormatConfig {
        FormatConfig::default()
    }

    #[test]
    fn formats_basic_structure() {
        let input = "module top;
initial begin
if(a)b<=c;
else
c<=d;
end
endmodule
";
        let expected = "\
module top;
  initial begin
    if (a) b <= c;
    else
    c <= d;
  end
endmodule
";
        let formatted = format_text(input, &cfg()).unwrap();
        let tree = parser::parse(input, &SvParserCfg::default()).unwrap();
        let align_map = collect_case_alignment(&tree);
        dbg!(&align_map);
        assert_eq!(formatted, expected);
    }

    #[test]
    fn aligns_preprocessor_left() {
        let input = "module x;
  `ifdef FOO
    assign a = b,c,d;
  `else
foo ( bar );
  `endif
endmodule
";
        let formatted = format_text(input, &cfg()).unwrap();
        for line in formatted.lines() {
            if line.starts_with('`') {
                assert!(!line.starts_with(" "), "directive must be left aligned: {line}");
            }
        }
    }

    #[test]
    fn call_and_comma_spacing() {
        let input = "module x;
initial begin
foo (a,b ,c);
end
endmodule
";
        let formatted = format_text(input, &cfg()).unwrap();
        assert!(formatted.contains("foo(a, b, c);"));
    }

    #[test]
    fn inline_end_else_one_line() {
        let input = "module x;
initial begin
if (a) begin
  do_something();
end
else begin
  other();
end
end
endmodule
";
        let formatted = format_text(input, &cfg()).unwrap();
        assert!(
            formatted.contains("end else begin"),
            "expected inline end else. got:\n{formatted}"
        );
    }

    #[test]
    fn wraps_multiline_blocks_when_enabled() {
        let input = "module x;
initial begin
if (cond)
  a <= 1;
  b <= 2;
end
endmodule
";
        let formatted = format_text(input, &cfg()).unwrap();
        let expected = "\
module x;
  initial begin
    if (cond)
    begin
      a <= 1;
      b <= 2;
    end
  end
endmodule
";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn does_not_wrap_case_statement_body() {
        let input = "module x;
always_comb begin
if (cond)
  case(sel)
    0: foo <= 1;
    default: foo <= 0;
  endcase
end
endmodule
";
        let formatted = format_text(input, &cfg()).unwrap();
        assert!(
            !formatted.contains("if (cond)\n    begin"),
            "case body should not trigger auto begin:\n{formatted}"
        );
    }

    #[test]
    fn comment_spacing_rules() {
        let input = "module x;
initial begin
//leading
assign a = 1;   //  trailing
/* block comment */
assign b = 2;
end
endmodule
";
        let formatted = format_text(input, &cfg()).unwrap();
        assert!(
            formatted.contains("  //leading"),
            "leading comment should only have indent:\n{formatted}"
        );
        assert!(
            formatted.contains("assign a = 1; //  trailing"),
            "inline comment should have a single separator space:\n{formatted}"
        );
        assert!(
            formatted.contains("\n\n    /* block comment */\n\n"),
            "block comment should be surrounded by blank lines:\n{formatted}"
        );
    }

    #[test]
    fn aligns_case_colons() {
        let input = "module x;
always_comb begin
case(sel)
  2'b0: foo = 0;
  4'b1010: foo = 1;
  default: foo = 2;
endcase
end
endmodule
";
        let formatted = format_text(input, &cfg()).unwrap();
        let short = formatted
            .lines()
            .find(|line| line.contains("foo = 0;"))
            .expect("missing short case item");
        assert!(
            short.contains("0    :"),
            "short label should be padded before colon:\n{formatted}"
        );
    }

    #[test]
    fn adds_blank_lines_around_declarations() {
        let input = "package demo;
class foo;
endclass
class bar;
endclass
endpackage
interface baz();
endinterface
";
        let formatted = format_text(input, &cfg()).unwrap();
        let expected = "\
package demo;

  class foo;
  endclass

  class bar;
  endclass
endpackage

interface baz();
  endinterface
";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn keeps_body_when_wrap_disabled() {
        let mut cfg = FormatConfig::default();
        cfg.wrap_multiline_blocks = false;
        let input = "module x;
initial begin
if (cond)
  a <= 1;
  b <= 2;
end
endmodule
";
        let formatted = format_text(input, &cfg).unwrap();
        assert!(
            !formatted.contains("if (cond)\n    begin"),
            "unexpected begin insertion:\n{formatted}"
        );
    }
}
