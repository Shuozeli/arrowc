pub mod arrow_type;
pub mod input;
pub mod schema_def;
pub mod validate;

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("parse error: {0}")]
    ParseError(String),

    #[error("invalid type: {0}")]
    InvalidType(String),

    #[error("validation error: {0}")]
    Validation(String),
}
