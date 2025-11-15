mod config;
mod formatter;
mod parser;

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use formatter::format_text;
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
    #[arg(long = "check")]
    check: bool,

    /// Path to a sv-fmt.toml configuration file.
    #[arg(long = "config", value_name = "PATH")]
    config: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.check && cli.in_place {
        bail!("--check and --in-place cannot be used together");
    }

    let config = config::load_config(cli.config.as_deref())?;
    let files = collect_files(&cli.paths)?;
    if files.is_empty() {
        bail!("no SystemVerilog files found to format");
    }

    if !cli.check && !cli.in_place && files.len() > 1 {
        bail!("formatting multiple files requires --in-place or --check");
    }

    let mut failed_paths = Vec::new();
    let mut lint_failures: Vec<(PathBuf, Vec<(usize, usize)>)> = Vec::new();

    for path in files {
        let original = read_input(&path)?;
        let formatted = format_text(&original, &config)?;
        let normalized = ensure_trailing_newline(&formatted);

        if cli.check {
            if normalized != ensure_trailing_newline(&original) {
                failed_paths.push(path.clone());
            }
            let violations = line_length_violations(&normalized, config.max_line_length);
            if !violations.is_empty() {
                lint_failures.push((path.clone(), violations));
            }
            continue;
        }

        if cli.in_place {
            if normalized != ensure_trailing_newline(&original) {
                fs::write(&path, normalized)?;
            }
        } else {
            io::stdout().write_all(normalized.as_bytes())?;
        }
    }

    if cli.check && (!failed_paths.is_empty() || !lint_failures.is_empty()) {
        for path in &failed_paths {
            eprintln!("needs formatting: {}", path.display());
        }
        for (path, lines) in &lint_failures {
            for (line, length) in lines {
                eprintln!(
                    "line {} has {} columns (max {}) in {}",
                    line,
                    length,
                    config.max_line_length,
                    path.display()
                );
            }
        }
        std::process::exit(1);
    }

    Ok(())
}

fn collect_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        let metadata = fs::metadata(path).with_context(|| format!("failed to read metadata for {}", path.display()))?;
        if metadata.is_dir() {
            for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
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
    let content = if bytes.starts_with(b"\xEF\xBB\xBF") {
        &bytes[3..]
    } else {
        &bytes
    };
    let mut text =
        String::from_utf8(content.to_vec()).with_context(|| format!("{} is not valid UTF-8", path.display()))?;
    text = normalize_newlines(&text);
    Ok(text)
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

fn line_length_violations(text: &str, max_len: usize) -> Vec<(usize, usize)> {
    if max_len == 0 {
        return Vec::new();
    }
    text.lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let cols = line.chars().count();
            if cols > max_len { Some((idx + 1, cols)) } else { None }
        })
        .collect()
}
