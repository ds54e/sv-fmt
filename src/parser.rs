use std::path::Path;

use anyhow::{Context, Result};
use sv_parser::{Defines, SyntaxTree, parse_sv_str};

#[derive(Debug, Clone)]
pub struct SvParserCfg {
    pub allow_incomplete: bool,
}

impl Default for SvParserCfg {
    fn default() -> Self {
        Self { allow_incomplete: true }
    }
}

pub fn parse(text: &str, cfg: &SvParserCfg) -> Result<SyntaxTree> {
    let defines: Defines = Defines::default();
    let include_paths: Vec<&Path> = Vec::new();

    let (tree, _) = parse_sv_str(
        text,
        Path::new("<memory>"),
        &defines,
        &include_paths,
        false,
        cfg.allow_incomplete,
    )
    .context("failed to parse SystemVerilog input")?;

    Ok(tree)
}
