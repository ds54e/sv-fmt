use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use sv_fmt::{config, formatter::format_text};
use walkdir::WalkDir;

#[derive(Debug, Parser)]
#[command(author, version, about = "SystemVerilog formatter")]
struct Cli {
    /// Files or directories to format.
    #[arg(value_name = "FILES", required = true)]
    paths: Vec<PathBuf>,

    /// Overwrite files in place.
    #[arg(short = 'i', long = "in-place")]
    in_place: bool,

    /// Only check if files are already formatted.
    #[arg(long = "check", conflicts_with = "in_place")]
    check: bool,

    /// Path to a sv-fmt.toml configuration file.
    #[arg(long = "config", value_name = "PATH")]
    config: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = config::load_config(cli.config.as_deref())?;
    let files = collect_files(&cli.paths)?;
    if files.is_empty() {
        bail!("no SystemVerilog files found to format");
    }

    if !cli.check && !cli.in_place && files.len() > 1 {
        bail!("formatting multiple files requires --in-place or --check");
    }

    let mut failed_paths = Vec::new();
    let mut lint_failures: Vec<(PathBuf, Vec<LineLengthViolation>)> = Vec::new();

    for path in files {
        let original = read_input(&path)?;
        let formatted = format_text(&original, &config)?;
        let normalized = ensure_trailing_newline(&formatted);
        let original_normalized = ensure_trailing_newline(&original);

        let violations = line_length_violations(&normalized, config.max_line_length);
        if !violations.is_empty() {
            lint_failures.push((path.clone(), violations));
        }

        if cli.check {
            if normalized != original_normalized {
                failed_paths.push(path.clone());
            }
            continue;
        }

        if cli.in_place {
            if normalized != original_normalized {
                fs::write(&path, normalized)?;
            }
        } else {
            io::stdout().write_all(normalized.as_bytes())?;
        }
    }

    if !failed_paths.is_empty() {
        for path in &failed_paths {
            eprintln!("needs formatting: {}", path.display());
        }
    }
    if !lint_failures.is_empty() {
        for (path, lines) in &lint_failures {
            for violation in lines {
                eprintln!(
                    "line {} has {} columns (max {}) in {}",
                    violation.line,
                    violation.columns,
                    config.max_line_length,
                    path.display()
                );
                eprintln!("    | {}", violation.preview);
                if config.max_line_length > 0 {
                    eprintln!("{}", caret_marker(&violation.preview, config.max_line_length));
                }
            }
        }
        eprintln!("hint: adjust max_line_length in sv-fmt.toml or via --config if needed");
    }
    if cli.check && (!failed_paths.is_empty() || !lint_failures.is_empty()) {
        std::process::exit(1);
    }
    if !cli.check && !lint_failures.is_empty() {
        bail!("line length violations detected; see output above");
    }

    Ok(())
}

fn collect_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        let metadata = fs::metadata(path).with_context(|| format!("failed to read metadata for {}", path.display()))?;
        if metadata.is_dir() {
            for entry in WalkDir::new(path) {
                let entry = entry.with_context(|| format!("failed to traverse {}", path.display()))?;
                if entry.file_type().is_file() && is_sv_file(entry.path()) {
                    files.push(entry.path().to_path_buf());
                }
            }
        } else if metadata.is_file() {
            if is_sv_file(path) {
                files.push(path.clone());
            }
        }
    }

    files.sort();
    files.dedup();
    Ok(files)
}

fn is_sv_file(path: &Path) -> bool {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => matches!(ext.to_ascii_lowercase().as_str(), "sv" | "svh" | "vh" | "v"),
        None => false,
    }
}

fn read_input(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut text = String::from_utf8(bytes).with_context(|| format!("{} is not valid UTF-8", path.display()))?;
    if text.starts_with('\u{feff}') {
        text.drain(..1);
    }
    Ok(normalize_newlines(&text))
}

fn normalize_newlines(input: &str) -> String {
    let mut normalized = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if matches!(chars.peek(), Some('\n')) {
                chars.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(ch);
        }
    }
    normalized
}

fn ensure_trailing_newline(text: &str) -> String {
    let mut result = text.to_string();
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

#[derive(Debug, Clone)]
struct LineLengthViolation {
    line: usize,
    columns: usize,
    preview: String,
}

fn line_length_violations(text: &str, max_len: usize) -> Vec<LineLengthViolation> {
    if max_len == 0 {
        return Vec::new();
    }
    text.lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let cols = line.chars().count();
            if cols > max_len {
                Some(LineLengthViolation {
                    line: idx + 1,
                    columns: cols,
                    preview: line_preview(line, max_len),
                })
            } else {
                None
            }
        })
        .collect()
}

fn line_preview(line: &str, max_len: usize) -> String {
    let limit = max_len.saturating_add(20).max(40);
    let mut preview = String::new();
    let mut count = 0;
    for ch in line.chars() {
        if count >= limit {
            preview.push_str("...");
            break;
        }
        preview.push(ch);
        count += 1;
    }
    preview
}

fn caret_marker(preview: &str, limit: usize) -> String {
    let target = limit.min(preview.chars().count());
    let mut marker = String::from("    | ");
    for ch in preview.chars().take(target) {
        marker.push(if ch == '\t' { '\t' } else { ' ' });
    }
    marker.push('^');
    marker.push_str(&format!(" column {}", limit + 1));
    marker
}
