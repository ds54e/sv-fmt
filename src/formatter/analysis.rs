use std::collections::HashMap;

use sv_parser::{Iter, NodeEvent, RefNode, RefNodes, SyntaxTree};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ByteSpan {
    pub(crate) end: usize,
}

pub(crate) fn collect_statement_spans(tree: &SyntaxTree) -> HashMap<usize, ByteSpan> {
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

pub(crate) fn collect_case_alignment(tree: &SyntaxTree) -> HashMap<usize, usize> {
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
    start.map(|_| ByteSpan { end })
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
