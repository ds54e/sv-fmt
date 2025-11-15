use std::collections::HashSet;

use once_cell::sync::Lazy;
use sv_parser::{NodeEvent, RefNode, SyntaxTree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
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
pub(crate) struct Token {
    pub(crate) text: String,
    pub(crate) kind: TokenKind,
    pub(crate) offset: usize,
    pub(crate) len: usize,
}

impl Token {
    pub(crate) fn new_spanned(text: impl Into<String>, kind: TokenKind, offset: usize, len: usize) -> Self {
        Self {
            text: text.into(),
            kind,
            offset,
            len,
        }
    }

    pub(crate) fn is_keyword(&self, needle: &str) -> bool {
        matches!(self.kind, TokenKind::Keyword) && self.text.eq_ignore_ascii_case(needle)
    }

    pub(crate) fn is_identifier_like(&self) -> bool {
        matches!(self.kind, TokenKind::Identifier)
    }

    pub(crate) fn is_symbol(&self, needle: &str) -> bool {
        matches!(self.kind, TokenKind::Symbol) && self.text == needle
    }

}

pub(crate) fn tokenize(tree: &SyntaxTree) -> Vec<Token> {
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
