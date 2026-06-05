use std::fs;
use std::path::Path;

use arrowc_schema::input;
use arrowc_schema::validate;

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("schema error: {0}")]
    Schema(#[from] arrowc_schema::SchemaError),

    #[error("{0}")]
    Other(String),
}

/// Supported input formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    Json,
    Yaml,
}

/// Detect input format from file extension, or return None.
pub fn detect_format(path: &Path) -> Option<InputFormat> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => Some(InputFormat::Json),
        Some("yaml") | Some("yml") => Some(InputFormat::Yaml),
        _ => None,
    }
}

/// Compile a schema file, generating Rust source files in the output directory.
pub fn compile(
    input_path: &Path,
    output_dir: &Path,
    format: Option<InputFormat>,
) -> Result<Vec<String>, CompileError> {
    let content = fs::read_to_string(input_path)?;

    let format = format
        .or_else(|| detect_format(input_path))
        .ok_or_else(|| {
            CompileError::Other(format!(
                "cannot detect format for '{}', use --format",
                input_path.display()
            ))
        })?;

    let schema_file = match format {
        InputFormat::Json => input::load_json(&content)?,
        InputFormat::Yaml => input::load_yaml(&content)?,
    };

    validate::validate(&schema_file)?;

    let files = arrowc_codegen::generate_rust(&schema_file);

    fs::create_dir_all(output_dir)?;

    let mut written = Vec::new();
    for (filename, content) in &files {
        let out_path = output_dir.join(filename);
        fs::write(&out_path, content)?;
        written.push(out_path.display().to_string());
    }

    Ok(written)
}
