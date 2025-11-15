use std::collections::HashSet;

use anyhow::Result;
use once_cell::sync::Lazy;
use sv_parser::{NodeEvent, RefNode, SyntaxTree};

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
}

impl Token {
    fn new(text: impl Into<String>, kind: TokenKind) -> Self {
        Self {
            text: text.into(),
            kind,
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
    let tokens = tokenize(&tree);
    let mut formatter = Formatter::new(config, tokens);
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
                        handle_locate(text, whitespace_depth, comment_depth, directive_depth, &mut tokens);
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
    whitespace_depth: usize,
    comment_depth: usize,
    directive_depth: usize,
    tokens: &mut Vec<Token>,
) {
    if text.is_empty() {
        return;
    }
    if comment_depth > 0 {
        tokens.push(Token::new(text, TokenKind::Comment));
        return;
    }
    if whitespace_depth > 0 {
        for ch in text.chars() {
            if ch == '\n' {
                tokens.push(Token::new("\n", TokenKind::Newline));
            }
        }
        return;
    }
    if directive_depth > 0 {
        tokens.push(Token::new(text, TokenKind::Directive));
        return;
    }

    tokens.push(Token::new(text, classify_token(text)));
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
    idx: usize,
    output: String,
    indent_level: usize,
    at_line_start: bool,
    pending_space: bool,
    previous_call_ident: bool,
    inserted_blocks: Vec<usize>,
    wrap_tracker: WrapTracker,
}

struct WrapTracker {
    mode: WrapMode,
    paren_depth: usize,
    keyword: Option<WrapKeyword>,
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
    fn new(config: &'a FormatConfig, tokens: Vec<Token>) -> Self {
        Self {
            config,
            tokens,
            idx: 0,
            output: String::new(),
            indent_level: 0,
            at_line_start: true,
            pending_space: false,
            previous_call_ident: false,
            inserted_blocks: Vec::new(),
            wrap_tracker: WrapTracker::new(),
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
        if self.at_line_start {
            self.write_indent();
        } else if !self.output.ends_with(' ') {
            self.output.push(' ');
        }
        self.output.push_str(token.text.trim_end_matches('\n'));
        if token.text.contains('\n') {
            self.output.push('\n');
            self.at_line_start = true;
        } else {
            self.pending_space = true;
        }
        self.previous_call_ident = false;
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

        if self.at_line_start {
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

        if self.config.wrap_multiline_blocks {
            self.wrap_tracker.maybe_start(&lowered);
        }
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
            | "fork"
            | "generate"
            | "interface"
    )
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
            | "endpackage"
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
        "fork",
        "join",
        "join_any",
        "join_none",
        "generate",
        "endgenerate",
        "interface",
        "endinterface",
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
        }
    }

    fn reset(&mut self) {
        self.mode = WrapMode::Idle;
        self.paren_depth = 0;
        self.keyword = None;
    }

    fn newline(&mut self) {}

    fn maybe_start(&mut self, keyword: &str) {
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

        let mut semicolons = 0usize;
        for token in tokens.iter().skip(index) {
            if matches!(token.kind, TokenKind::Newline) {
                continue;
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
                if semicolons >= 2 {
                    break;
                }
            }
        }

        semicolons >= 2
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
