use crate::config::FormatConfig;

pub(crate) fn wrap_formatted_output(text: String, config: &FormatConfig) -> String {
    if config.max_line_length == 0 {
        return text;
    }
    let mut result = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (body, has_newline) = if line.ends_with('\n') {
            (&line[..line.len() - 1], true)
        } else {
            (line, false)
        };

        if body.is_empty() && has_newline {
            result.push('\n');
            continue;
        }

        let segments = wrap_line(body, config);
        for segment in segments {
            result.push_str(&segment);
            result.push('\n');
        }

        if !has_newline && !line.is_empty() && !text.ends_with('\n') {
            result.pop();
        }
    }
    // Preserve trailing newline behavior of the formatter by ensuring we end with '\n'.
    if !result.ends_with('\n') && text.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn wrap_line(line: &str, config: &FormatConfig) -> Vec<String> {
    if config.max_line_length == 0 || line.chars().count() <= config.max_line_length {
        return vec![line.to_string()];
    }
    let trimmed = line.trim_start();
    if trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with('`')
    {
        return vec![line.to_string()];
    }
    let indent_bytes = line.len() - trimmed.len();
    let indent = &line[..indent_bytes];
    let continuation_indent = if config.use_tabs {
        format!("{indent}\t")
    } else {
        format!("{indent}{}", " ".repeat(config.indent_width))
    };

    let mut segments = Vec::new();
    let mut current: Vec<char> = indent.chars().collect();
    let mut columns = current.len();
    let mut last_wrap_ix: Option<usize> = None;

    for ch in trimmed.chars() {
        current.push(ch);
        columns += 1;
        if ch.is_whitespace() || matches!(ch, ',' | ';' | '+' | '-' | '*' | '/' | '&' | '|' | '=') {
            last_wrap_ix = Some(current.len());
        }

        if columns > config.max_line_length {
            if let Some(ix) = last_wrap_ix {
                let (head, tail) = current.split_at(ix);
                let mut head_str: String = head.iter().collect();
                if !head_str.trim().is_empty() {
                    head_str = head_str.trim_end().to_string();
                }
                if !head_str.is_empty() {
                    segments.push(head_str);
                }
                let mut new_chars: Vec<char> = continuation_indent.chars().collect();
                let trimmed_tail: Vec<char> = tail.iter().skip_while(|c| c.is_ascii_whitespace()).cloned().collect();
                new_chars.extend(trimmed_tail);
                columns = new_chars.len();
                current = new_chars;
                last_wrap_ix = None;
            } else {
                break;
            }
        }
    }

    segments.push(current.iter().collect());
    segments
}
