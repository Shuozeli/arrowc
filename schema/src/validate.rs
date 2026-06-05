use std::collections::HashSet;

use crate::arrow_type::{ArrowType, TimeUnit};
use crate::schema_def::{ArrowFieldDef, ArrowSchemaFile};
use crate::SchemaError;

/// Validate an Arrow schema file for common errors.
pub fn validate(file: &ArrowSchemaFile) -> Result<(), SchemaError> {
    validate_no_duplicate_schema_names(file)?;
    for schema in &file.schemas {
        if schema.name.is_empty() {
            return Err(SchemaError::Validation(
                "schema name must not be empty".into(),
            ));
        }
        validate_no_duplicate_field_names(&schema.name, &schema.fields)?;
        for field in &schema.fields {
            validate_field_recursive(&schema.name, field)?;
        }
    }
    Ok(())
}

fn validate_no_duplicate_schema_names(file: &ArrowSchemaFile) -> Result<(), SchemaError> {
    let mut seen = HashSet::new();
    for schema in &file.schemas {
        if !seen.insert(&schema.name) {
            return Err(SchemaError::Validation(format!(
                "duplicate schema name: {}",
                schema.name
            )));
        }
    }
    Ok(())
}

fn validate_no_duplicate_field_names(
    context: &str,
    fields: &[ArrowFieldDef],
) -> Result<(), SchemaError> {
    let mut seen = HashSet::new();
    for field in fields {
        if !seen.insert(&field.name) {
            return Err(SchemaError::Validation(format!(
                "duplicate field name '{}' in '{context}'",
                field.name
            )));
        }
    }
    Ok(())
}

/// Recursively validate a field and its nested types.
fn validate_field_recursive(context: &str, field: &ArrowFieldDef) -> Result<(), SchemaError> {
    if field.name.is_empty() {
        return Err(SchemaError::Validation(format!(
            "field name must not be empty in '{context}'"
        )));
    }
    validate_arrow_type(context, &field.name, &field.arrow_type)?;
    Ok(())
}

/// Validate Arrow type constraints.
fn validate_arrow_type(context: &str, field_name: &str, ty: &ArrowType) -> Result<(), SchemaError> {
    let ctx = format!("{context}.{field_name}");
    match ty {
        ArrowType::Decimal128 { precision, scale } => {
            if *precision == 0 || *precision > 38 {
                return Err(SchemaError::Validation(format!(
                    "decimal128 precision must be 1..=38, got {precision} in '{ctx}'"
                )));
            }
            if *scale < 0 || *scale as u8 > *precision {
                return Err(SchemaError::Validation(format!(
                    "decimal128 scale must be 0..={precision}, got {scale} in '{ctx}'"
                )));
            }
        }
        ArrowType::Decimal256 { precision, scale } => {
            if *precision == 0 || *precision > 76 {
                return Err(SchemaError::Validation(format!(
                    "decimal256 precision must be 1..=76, got {precision} in '{ctx}'"
                )));
            }
            if *scale < 0 || *scale as u8 > *precision {
                return Err(SchemaError::Validation(format!(
                    "decimal256 scale must be 0..={precision}, got {scale} in '{ctx}'"
                )));
            }
        }
        ArrowType::Time32 { unit } => match unit {
            TimeUnit::Second | TimeUnit::Millisecond => {}
            _ => {
                return Err(SchemaError::Validation(format!(
                    "time32 only supports Second or Millisecond, got {unit} in '{ctx}'"
                )));
            }
        },
        ArrowType::Time64 { unit } => match unit {
            TimeUnit::Microsecond | TimeUnit::Nanosecond => {}
            _ => {
                return Err(SchemaError::Validation(format!(
                    "time64 only supports Microsecond or Nanosecond, got {unit} in '{ctx}'"
                )));
            }
        },
        ArrowType::FixedSizeBinary { byte_width } if *byte_width <= 0 => {
            return Err(SchemaError::Validation(format!(
                "fixed_size_binary byte_width must be positive, got {byte_width} in '{ctx}'"
            )));
        }
        ArrowType::FixedSizeBinary { .. } => {}
        ArrowType::FixedSizeList { element, size } => {
            if *size <= 0 {
                return Err(SchemaError::Validation(format!(
                    "fixed_size_list size must be positive, got {size} in '{ctx}'"
                )));
            }
            validate_field_recursive(&ctx, element)?;
        }
        ArrowType::Struct { fields } => {
            if fields.is_empty() {
                return Err(SchemaError::Validation(format!(
                    "struct must have at least one field in '{ctx}'"
                )));
            }
            validate_no_duplicate_field_names(&ctx, fields)?;
            for f in fields {
                validate_field_recursive(&ctx, f)?;
            }
        }
        ArrowType::List { element } | ArrowType::LargeList { element } => {
            validate_field_recursive(&ctx, element)?;
        }
        ArrowType::Map { key, value, .. } => {
            validate_field_recursive(&ctx, key)?;
            validate_field_recursive(&ctx, value)?;
        }
        ArrowType::Union { fields, .. } => {
            if fields.is_empty() {
                return Err(SchemaError::Validation(format!(
                    "union must have at least one field in '{ctx}'"
                )));
            }
            validate_no_duplicate_field_names(&ctx, fields)?;
            for f in fields {
                validate_field_recursive(&ctx, f)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arrow_type::ArrowType;
    use crate::schema_def::{ArrowFieldDef, ArrowSchemaDef};
    use std::collections::BTreeMap;

    fn schema_file(name: &str, fields: Vec<ArrowFieldDef>) -> ArrowSchemaFile {
        ArrowSchemaFile {
            schemas: vec![ArrowSchemaDef {
                name: name.into(),
                fields,
                metadata: BTreeMap::new(),
            }],
        }
    }

    // --- Existing tests (preserved) ---

    #[test]
    fn test_validate_passes_for_valid_schema() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![
                ArrowFieldDef::new("id", ArrowType::Int64),
                ArrowFieldDef::new("name", ArrowType::Utf8),
            ],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_rejects_duplicate_schema_names() {
        // Arrange
        let file = ArrowSchemaFile {
            schemas: vec![
                ArrowSchemaDef {
                    name: "Test".into(),
                    fields: vec![],
                    metadata: BTreeMap::new(),
                },
                ArrowSchemaDef {
                    name: "Test".into(),
                    fields: vec![],
                    metadata: BTreeMap::new(),
                },
            ],
        };

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate schema name"), "{err}");
    }

    #[test]
    fn test_validate_rejects_duplicate_field_names() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![
                ArrowFieldDef::new("id", ArrowType::Int64),
                ArrowFieldDef::new("id", ArrowType::Utf8),
            ],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate field name"), "{err}");
    }

    // --- New validation tests ---

    #[test]
    fn test_validate_rejects_empty_schema_name() {
        // Arrange
        let file = schema_file("", vec![ArrowFieldDef::new("id", ArrowType::Int64)]);

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("schema name must not be empty"));
    }

    #[test]
    fn test_validate_rejects_empty_field_name() {
        // Arrange
        let file = schema_file("Test", vec![ArrowFieldDef::new("", ArrowType::Int64)]);

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("field name must not be empty"));
    }

    #[test]
    fn test_validate_rejects_decimal128_precision_zero() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "x",
                ArrowType::Decimal128 {
                    precision: 0,
                    scale: 0,
                },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("precision must be 1..=38"));
    }

    #[test]
    fn test_validate_rejects_decimal128_precision_over_38() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "x",
                ArrowType::Decimal128 {
                    precision: 39,
                    scale: 0,
                },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("precision must be 1..=38"));
    }

    #[test]
    fn test_validate_rejects_decimal128_scale_exceeds_precision() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "x",
                ArrowType::Decimal128 {
                    precision: 2,
                    scale: 10,
                },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("scale must be 0..=2"));
    }

    #[test]
    fn test_validate_accepts_valid_decimal128() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "x",
                ArrowType::Decimal128 {
                    precision: 10,
                    scale: 2,
                },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_rejects_time32_microsecond() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "t",
                ArrowType::Time32 {
                    unit: TimeUnit::Microsecond,
                },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("time32 only supports Second or Millisecond"));
    }

    #[test]
    fn test_validate_rejects_time32_nanosecond() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "t",
                ArrowType::Time32 {
                    unit: TimeUnit::Nanosecond,
                },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_rejects_time64_second() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "t",
                ArrowType::Time64 {
                    unit: TimeUnit::Second,
                },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("time64 only supports Microsecond or Nanosecond"));
    }

    #[test]
    fn test_validate_rejects_time64_millisecond() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "t",
                ArrowType::Time64 {
                    unit: TimeUnit::Millisecond,
                },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_accepts_valid_time32_second() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "t",
                ArrowType::Time32 {
                    unit: TimeUnit::Second,
                },
            )],
        );

        // Act & Assert
        assert!(validate(&file).is_ok());
    }

    #[test]
    fn test_validate_accepts_valid_time64_microsecond() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "t",
                ArrowType::Time64 {
                    unit: TimeUnit::Microsecond,
                },
            )],
        );

        // Act & Assert
        assert!(validate(&file).is_ok());
    }

    #[test]
    fn test_validate_rejects_empty_struct_fields() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "s",
                ArrowType::Struct { fields: vec![] },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("at least one field"));
    }

    #[test]
    fn test_validate_rejects_duplicate_struct_subfield_names() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "s",
                ArrowType::Struct {
                    fields: vec![
                        ArrowFieldDef::new("x", ArrowType::Int32),
                        ArrowFieldDef::new("x", ArrowType::Float64),
                    ],
                },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("duplicate field name"));
    }

    #[test]
    fn test_validate_nested_list_struct_validation() {
        // Arrange — list<struct> with invalid decimal inside
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "items",
                ArrowType::List {
                    element: Box::new(ArrowFieldDef::new(
                        "item",
                        ArrowType::Struct {
                            fields: vec![ArrowFieldDef::new(
                                "bad",
                                ArrowType::Decimal128 {
                                    precision: 0,
                                    scale: 0,
                                },
                            )],
                        },
                    )),
                },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("precision must be 1..=38"));
    }

    #[test]
    fn test_validate_rejects_negative_fixed_size_binary() {
        // Arrange
        let file = schema_file(
            "Test",
            vec![ArrowFieldDef::new(
                "b",
                ArrowType::FixedSizeBinary { byte_width: -1 },
            )],
        );

        // Act
        let result = validate(&file);

        // Assert
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("byte_width must be positive"));
    }
}
