use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FormatConfig {
    pub indent_width: usize,
    pub use_tabs: bool,
    pub align_preprocessor: bool,
    pub wrap_multiline_blocks: bool,
    pub inline_end_else: bool,
    pub space_after_comma: bool,
    pub remove_call_space: bool,
    pub max_line_length: usize,
    pub align_case_colon: bool,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent_width: 2,
            use_tabs: false,
            align_preprocessor: true,
            wrap_multiline_blocks: true,
            inline_end_else: true,
            space_after_comma: true,
            remove_call_space: true,
            max_line_length: 100,
            align_case_colon: true,
        }
    }
}

pub fn load_config(path: Option<&Path>) -> Result<FormatConfig> {
    if let Some(path) = path {
        return read_config_file(path);
    }

    let default_path = PathBuf::from("sv-fmt.toml");
    if default_path.exists() {
        return read_config_file(&default_path);
    }

    Ok(FormatConfig::default())
}

fn read_config_file(path: &Path) -> Result<FormatConfig> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read config file {}", path.display()))?;
    let mut config: FormatConfig =
        toml::from_str(&contents).with_context(|| format!("invalid config file {}", path.display()))?;

    // Guard against invalid zero widths so formatter never panics later.
    if config.indent_width == 0 {
        config.indent_width = 2;
    }

    Ok(config)
}
