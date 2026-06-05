use std::collections::BTreeMap;

use serde::Deserialize;

use crate::arrow_type::{ArrowType, IntervalUnit, TimeUnit, UnionMode};
use crate::schema_def::{ArrowFieldDef, ArrowSchemaDef, ArrowSchemaFile};
use crate::SchemaError;

// ---------------------------------------------------------------------------
// Serde input models
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct InputSchemaFile {
    #[allow(dead_code)]
    pub version: u32,
    pub schemas: Vec<InputSchema>,
}

#[derive(Deserialize)]
pub struct InputSchema {
    pub name: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    pub fields: Vec<InputField>,
}

#[derive(Deserialize)]
pub struct InputField {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: InputType,
    #[serde(default = "default_true")]
    pub nullable: bool,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

fn default_true() -> bool {
    true
}

/// The `type` field can be a string (`"int32"`, `"list<utf8>"`) or an inline
/// struct definition (YAML mapping with a `struct` key).
#[derive(Deserialize)]
#[serde(untagged)]
pub enum InputType {
    Simple(String),
    InlineStruct {
        #[serde(rename = "struct")]
        fields: Vec<InputField>,
    },
}

// ---------------------------------------------------------------------------
// Conversion to IR
// ---------------------------------------------------------------------------

impl InputSchemaFile {
    pub fn into_ir(self) -> Result<ArrowSchemaFile, SchemaError> {
        let schemas = self
            .schemas
            .into_iter()
            .map(|s| s.into_ir())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ArrowSchemaFile { schemas })
    }
}

impl InputSchema {
    fn into_ir(self) -> Result<ArrowSchemaDef, SchemaError> {
        let fields = self
            .fields
            .into_iter()
            .map(|f| f.into_ir())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ArrowSchemaDef {
            name: self.name,
            fields,
            metadata: self.metadata,
        })
    }
}

impl InputField {
    fn into_ir(self) -> Result<ArrowFieldDef, SchemaError> {
        let arrow_type = match self.type_ {
            InputType::Simple(ref s) => parse_type_string(s)?,
            InputType::InlineStruct { fields } => {
                let ir_fields = fields
                    .into_iter()
                    .map(|f| f.into_ir())
                    .collect::<Result<Vec<_>, _>>()?;
                ArrowType::Struct { fields: ir_fields }
            }
        };
        Ok(ArrowFieldDef {
            name: self.name,
            arrow_type,
            nullable: self.nullable,
            metadata: self.metadata,
        })
    }
}

// ---------------------------------------------------------------------------
// Type string parser
// ---------------------------------------------------------------------------

/// Parse a type string like `"int32"`, `"timestamp[us, UTC]"`, `"list<utf8>"`,
/// `"decimal128(10, 2)"`, `"map<utf8, int32>"`.
pub fn parse_type_string(input: &str) -> Result<ArrowType, SchemaError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(SchemaError::InvalidType("empty type string".into()));
    }

    // Try generic types with angle brackets: list<T>, large_list<T>,
    // fixed_size_list<T>(N), map<K, V>, dictionary<I, V>
    if let Some(result) = try_parse_generic(s)? {
        return Ok(result);
    }

    // Try parameterized types with square brackets: timestamp[us, UTC], etc.
    if let Some(result) = try_parse_bracketed(s)? {
        return Ok(result);
    }

    // Try types with parentheses: decimal128(10, 2), fixed_size_binary(256)
    if let Some(result) = try_parse_parens(s)? {
        return Ok(result);
    }

    // Simple bare type names
    parse_bare_type(s)
}

/// Parse bare type names without any parameters.
fn parse_bare_type(s: &str) -> Result<ArrowType, SchemaError> {
    match s {
        "null" => Ok(ArrowType::Null),
        "bool" | "boolean" => Ok(ArrowType::Boolean),
        "int8" => Ok(ArrowType::Int8),
        "int16" => Ok(ArrowType::Int16),
        "int32" => Ok(ArrowType::Int32),
        "int64" => Ok(ArrowType::Int64),
        "uint8" => Ok(ArrowType::UInt8),
        "uint16" => Ok(ArrowType::UInt16),
        "uint32" => Ok(ArrowType::UInt32),
        "uint64" => Ok(ArrowType::UInt64),
        "float16" | "half" => Ok(ArrowType::Float16),
        "float32" | "float" => Ok(ArrowType::Float32),
        "float64" | "double" => Ok(ArrowType::Float64),
        "utf8" | "string" => Ok(ArrowType::Utf8),
        "large_utf8" | "large_string" => Ok(ArrowType::LargeUtf8),
        "binary" => Ok(ArrowType::Binary),
        "large_binary" => Ok(ArrowType::LargeBinary),
        "date32" => Ok(ArrowType::Date32),
        "date64" => Ok(ArrowType::Date64),
        _ => Err(SchemaError::InvalidType(format!("unknown type: {s}"))),
    }
}

/// Try parsing generic types: `list<T>`, `large_list<T>`, `fixed_size_list<T>(N)`,
/// `map<K, V>`, `dictionary<I, V>`.
fn try_parse_generic(s: &str) -> Result<Option<ArrowType>, SchemaError> {
    let Some(angle_start) = s.find('<') else {
        return Ok(None);
    };

    // Find the matching closing '>'
    let prefix = &s[..angle_start];
    let angle_end = find_matching_close(s, angle_start, '<', '>')?;

    let inner = &s[angle_start + 1..angle_end];

    match prefix {
        "list" => {
            let elem_type = parse_type_string(inner)?;
            let element = Box::new(ArrowFieldDef::new("item", elem_type));
            Ok(Some(ArrowType::List { element }))
        }
        "large_list" => {
            let elem_type = parse_type_string(inner)?;
            let element = Box::new(ArrowFieldDef::new("item", elem_type));
            Ok(Some(ArrowType::LargeList { element }))
        }
        "fixed_size_list" => {
            let elem_type = parse_type_string(inner)?;
            let element = Box::new(ArrowFieldDef::new("item", elem_type));
            // Expect (N) after the >
            let rest = &s[angle_end + 1..];
            let size = parse_paren_int(rest, "fixed_size_list")?;
            if size <= 0 {
                return Err(SchemaError::InvalidType(format!(
                    "fixed_size_list size must be positive, got {size}"
                )));
            }
            Ok(Some(ArrowType::FixedSizeList { element, size }))
        }
        "map" => {
            let (key_str, value_str) = split_top_level_comma(inner)?;
            let key_type = parse_type_string(key_str)?;
            let value_type = parse_type_string(value_str)?;
            let key = Box::new(ArrowFieldDef::new("key", key_type).with_nullable(false));
            let value = Box::new(ArrowFieldDef::new("value", value_type));
            Ok(Some(ArrowType::Map {
                key,
                value,
                keys_sorted: false,
            }))
        }
        "dictionary" => {
            let (index_str, value_str) = split_top_level_comma(inner)?;
            let index_type = Box::new(parse_type_string(index_str)?);
            let value_type = Box::new(parse_type_string(value_str)?);
            Ok(Some(ArrowType::Dictionary {
                index_type,
                value_type,
            }))
        }
        "struct" => {
            // struct<name: type, name: type, ...>
            let fields = parse_named_fields(inner, "struct")?;
            Ok(Some(ArrowType::Struct { fields }))
        }
        "union" => {
            // union<field1: type1, field2: type2>
            let fields = parse_named_fields(inner, "union")?;
            Ok(Some(ArrowType::Union {
                fields,
                mode: UnionMode::Dense,
            }))
        }
        _ => Err(SchemaError::InvalidType(format!(
            "unknown generic type: {prefix}"
        ))),
    }
}

/// Try parsing types with square brackets: `timestamp[us, UTC]`, `time32[ms]`,
/// `duration[ns]`, `interval[year_month]`.
fn try_parse_bracketed(s: &str) -> Result<Option<ArrowType>, SchemaError> {
    let Some(bracket_start) = s.find('[') else {
        return Ok(None);
    };
    let Some(bracket_end) = s.rfind(']') else {
        return Err(SchemaError::InvalidType(format!(
            "unmatched '[' in type: {s}"
        )));
    };

    let prefix = &s[..bracket_start];
    let params = &s[bracket_start + 1..bracket_end];

    match prefix {
        "timestamp" => {
            let parts: Vec<&str> = params.splitn(2, ',').collect();
            let unit = TimeUnit::parse(parts[0]).ok_or_else(|| {
                SchemaError::InvalidType(format!("invalid time unit: {}", parts[0]))
            })?;
            let timezone = parts.get(1).map(|tz| tz.trim().to_string());
            Ok(Some(ArrowType::Timestamp { unit, timezone }))
        }
        "time32" => {
            let unit = TimeUnit::parse(params)
                .ok_or_else(|| SchemaError::InvalidType(format!("invalid time unit: {params}")))?;
            Ok(Some(ArrowType::Time32 { unit }))
        }
        "time64" => {
            let unit = TimeUnit::parse(params)
                .ok_or_else(|| SchemaError::InvalidType(format!("invalid time unit: {params}")))?;
            Ok(Some(ArrowType::Time64 { unit }))
        }
        "duration" => {
            let unit = TimeUnit::parse(params)
                .ok_or_else(|| SchemaError::InvalidType(format!("invalid time unit: {params}")))?;
            Ok(Some(ArrowType::Duration { unit }))
        }
        "interval" => {
            let unit = match params.trim() {
                "year_month" => IntervalUnit::YearMonth,
                "day_time" => IntervalUnit::DayTime,
                "month_day_nano" => IntervalUnit::MonthDayNano,
                _ => {
                    return Err(SchemaError::InvalidType(format!(
                        "invalid interval unit: {params}"
                    )))
                }
            };
            Ok(Some(ArrowType::Interval { unit }))
        }
        _ => Err(SchemaError::InvalidType(format!(
            "unknown bracketed type: {prefix}"
        ))),
    }
}

/// Try parsing types with parentheses: `decimal128(10, 2)`, `decimal256(38, 10)`,
/// `fixed_size_binary(256)`.
fn try_parse_parens(s: &str) -> Result<Option<ArrowType>, SchemaError> {
    let Some(paren_start) = s.find('(') else {
        return Ok(None);
    };
    let Some(paren_end) = s.rfind(')') else {
        return Err(SchemaError::InvalidType(format!(
            "unmatched '(' in type: {s}"
        )));
    };

    let prefix = &s[..paren_start];
    let params = &s[paren_start + 1..paren_end];

    match prefix {
        "decimal128" => {
            let (precision, scale) = parse_precision_scale(params)?;
            Ok(Some(ArrowType::Decimal128 { precision, scale }))
        }
        "decimal256" => {
            let (precision, scale) = parse_precision_scale(params)?;
            Ok(Some(ArrowType::Decimal256 { precision, scale }))
        }
        "fixed_size_binary" => {
            let byte_width: i32 = params.trim().parse().map_err(|_| {
                SchemaError::InvalidType(format!(
                    "invalid byte_width for fixed_size_binary: {params}"
                ))
            })?;
            if byte_width <= 0 {
                return Err(SchemaError::InvalidType(format!(
                    "fixed_size_binary byte_width must be positive, got {byte_width}"
                )));
            }
            Ok(Some(ArrowType::FixedSizeBinary { byte_width }))
        }
        _ => Err(SchemaError::InvalidType(format!(
            "unknown parameterized type: {prefix}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse `name: type, name: type` inside angle brackets for struct/union.
fn parse_named_fields(inner: &str, context: &str) -> Result<Vec<ArrowFieldDef>, SchemaError> {
    let parts = split_all_top_level_commas(inner)?;
    parts
        .into_iter()
        .map(|part| {
            let part = part.trim();
            // Split on first colon only — the type itself may contain colons
            // in nested struct<...> but split_once(':') stops at the first one
            if let Some((name, type_str)) = part.split_once(':') {
                let t = parse_type_string(type_str.trim())?;
                Ok(ArrowFieldDef::new(name.trim(), t))
            } else {
                Err(SchemaError::InvalidType(format!(
                    "{context} field must be 'name: type', got: {part}"
                )))
            }
        })
        .collect()
}

fn parse_precision_scale(params: &str) -> Result<(u8, i8), SchemaError> {
    let parts: Vec<&str> = params.split(',').collect();
    if parts.len() != 2 {
        return Err(SchemaError::InvalidType(format!(
            "decimal requires (precision, scale), got: ({params})"
        )));
    }
    // Parse into wider types first to detect overflow with clear messages.
    let p: u32 = parts[0]
        .trim()
        .parse()
        .map_err(|_| SchemaError::InvalidType(format!("invalid precision: {}", parts[0].trim())))?;
    let s: i32 = parts[1]
        .trim()
        .parse()
        .map_err(|_| SchemaError::InvalidType(format!("invalid scale: {}", parts[1].trim())))?;
    if p > u8::MAX as u32 {
        return Err(SchemaError::InvalidType(format!(
            "decimal precision {p} out of range (max {})",
            u8::MAX
        )));
    }
    if s < i8::MIN as i32 || s > i8::MAX as i32 {
        return Err(SchemaError::InvalidType(format!(
            "decimal scale {s} out of range ({}..={})",
            i8::MIN,
            i8::MAX
        )));
    }
    Ok((p as u8, s as i8))
}

fn parse_paren_int(s: &str, context: &str) -> Result<i32, SchemaError> {
    let s = s.trim();
    if !s.starts_with('(') || !s.ends_with(')') {
        return Err(SchemaError::InvalidType(format!(
            "{context} requires (N) suffix, got: {s}"
        )));
    }
    let inner = &s[1..s.len() - 1];
    inner
        .trim()
        .parse()
        .map_err(|_| SchemaError::InvalidType(format!("{context}: invalid size: {inner}")))
}

/// Find the matching closing delimiter, accounting for nesting.
fn find_matching_close(
    s: &str,
    start: usize,
    open: char,
    close: char,
) -> Result<usize, SchemaError> {
    let mut depth = 0;
    for (i, c) in s[start..].char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Ok(start + i);
            }
        }
    }
    Err(SchemaError::InvalidType(format!(
        "unmatched '{open}' in type: {s}"
    )))
}

/// Split at the first top-level comma (respecting nested angle brackets).
fn split_top_level_comma(s: &str) -> Result<(&str, &str), SchemaError> {
    let parts = split_all_top_level_commas(s)?;
    if parts.len() != 2 {
        return Err(SchemaError::InvalidType(format!(
            "expected exactly 2 comma-separated type arguments, got {}: {s}",
            parts.len()
        )));
    }
    // Since the parts are slices into `s`, we need to find the split point
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                return Ok((&s[..i], &s[i + 1..]));
            }
            _ => {}
        }
    }
    Err(SchemaError::InvalidType(format!("no comma found in: {s}")))
}

/// Split at all top-level commas (respecting nested delimiters).
fn split_all_top_level_commas(s: &str) -> Result<Vec<&str>, SchemaError> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    Ok(parts)
}

// ---------------------------------------------------------------------------
// Public API for loading schema files
// ---------------------------------------------------------------------------

/// Load an Arrow schema file from a JSON string.
pub fn load_json(json: &str) -> Result<ArrowSchemaFile, SchemaError> {
    let input: InputSchemaFile =
        serde_json::from_str(json).map_err(|e| SchemaError::ParseError(e.to_string()))?;
    input.into_ir()
}

/// Load an Arrow schema file from a YAML string.
pub fn load_yaml(yaml: &str) -> Result<ArrowSchemaFile, SchemaError> {
    let input: InputSchemaFile =
        serde_yaml::from_str(yaml).map_err(|e| SchemaError::ParseError(e.to_string()))?;
    input.into_ir()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bare_scalars() {
        assert_eq!(parse_type_string("int32").unwrap(), ArrowType::Int32);
        assert_eq!(parse_type_string("float64").unwrap(), ArrowType::Float64);
        assert_eq!(parse_type_string("utf8").unwrap(), ArrowType::Utf8);
        assert_eq!(parse_type_string("string").unwrap(), ArrowType::Utf8);
        assert_eq!(parse_type_string("boolean").unwrap(), ArrowType::Boolean);
        assert_eq!(parse_type_string("bool").unwrap(), ArrowType::Boolean);
        assert_eq!(parse_type_string("date32").unwrap(), ArrowType::Date32);
        assert_eq!(parse_type_string("binary").unwrap(), ArrowType::Binary);
        assert_eq!(
            parse_type_string("large_utf8").unwrap(),
            ArrowType::LargeUtf8
        );
    }

    #[test]
    fn test_parse_timestamp() {
        // Arrange
        let input = "timestamp[us, UTC]";

        // Act
        let result = parse_type_string(input).unwrap();

        // Assert
        assert_eq!(
            result,
            ArrowType::Timestamp {
                unit: TimeUnit::Microsecond,
                timezone: Some("UTC".into()),
            }
        );
    }

    #[test]
    fn test_parse_timestamp_without_timezone() {
        // Arrange
        let input = "timestamp[ns]";

        // Act
        let result = parse_type_string(input).unwrap();

        // Assert
        assert_eq!(
            result,
            ArrowType::Timestamp {
                unit: TimeUnit::Nanosecond,
                timezone: None,
            }
        );
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(
            parse_type_string("duration[ms]").unwrap(),
            ArrowType::Duration {
                unit: TimeUnit::Millisecond
            }
        );
    }

    #[test]
    fn test_parse_decimal128() {
        // Arrange
        let input = "decimal128(10, 2)";

        // Act
        let result = parse_type_string(input).unwrap();

        // Assert
        assert_eq!(
            result,
            ArrowType::Decimal128 {
                precision: 10,
                scale: 2
            }
        );
    }

    #[test]
    fn test_parse_fixed_size_binary() {
        assert_eq!(
            parse_type_string("fixed_size_binary(256)").unwrap(),
            ArrowType::FixedSizeBinary { byte_width: 256 }
        );
    }

    #[test]
    fn test_parse_list() {
        // Arrange
        let input = "list<utf8>";

        // Act
        let result = parse_type_string(input).unwrap();

        // Assert
        match result {
            ArrowType::List { element } => {
                assert_eq!(element.name, "item");
                assert_eq!(element.arrow_type, ArrowType::Utf8);
                assert!(element.nullable);
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_nested_list() {
        // Arrange
        let input = "list<list<int32>>";

        // Act
        let result = parse_type_string(input).unwrap();

        // Assert
        match result {
            ArrowType::List { element } => match &element.arrow_type {
                ArrowType::List { element: inner } => {
                    assert_eq!(inner.arrow_type, ArrowType::Int32);
                }
                other => panic!("expected inner List, got {other:?}"),
            },
            other => panic!("expected outer List, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_map() {
        // Arrange
        let input = "map<utf8, int32>";

        // Act
        let result = parse_type_string(input).unwrap();

        // Assert
        match result {
            ArrowType::Map {
                key,
                value,
                keys_sorted,
            } => {
                assert_eq!(key.arrow_type, ArrowType::Utf8);
                assert!(!key.nullable);
                assert_eq!(value.arrow_type, ArrowType::Int32);
                assert!(!keys_sorted);
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_dictionary() {
        // Arrange
        let input = "dictionary<int8, utf8>";

        // Act
        let result = parse_type_string(input).unwrap();

        // Assert
        assert_eq!(
            result,
            ArrowType::Dictionary {
                index_type: Box::new(ArrowType::Int8),
                value_type: Box::new(ArrowType::Utf8),
            }
        );
    }

    #[test]
    fn test_parse_list_of_timestamp() {
        // Arrange
        let input = "list<timestamp[us, UTC]>";

        // Act
        let result = parse_type_string(input).unwrap();

        // Assert
        match result {
            ArrowType::List { element } => {
                assert_eq!(
                    element.arrow_type,
                    ArrowType::Timestamp {
                        unit: TimeUnit::Microsecond,
                        timezone: Some("UTC".into()),
                    }
                );
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn test_load_yaml_basic() {
        // Arrange
        let yaml = r#"
version: 1
schemas:
  - name: Test
    fields:
      - name: id
        type: int64
        nullable: false
      - name: name
        type: utf8
      - name: score
        type: float64
"#;

        // Act
        let file = load_yaml(yaml).unwrap();

        // Assert
        assert_eq!(file.schemas.len(), 1);
        let schema = &file.schemas[0];
        assert_eq!(schema.name, "Test");
        assert_eq!(schema.fields.len(), 3);
        assert_eq!(schema.fields[0].name, "id");
        assert_eq!(schema.fields[0].arrow_type, ArrowType::Int64);
        assert!(!schema.fields[0].nullable);
        assert_eq!(schema.fields[1].name, "name");
        assert_eq!(schema.fields[1].arrow_type, ArrowType::Utf8);
        assert!(schema.fields[1].nullable);
    }

    #[test]
    fn test_load_yaml_inline_struct() {
        // Arrange
        let yaml = r#"
version: 1
schemas:
  - name: WithStruct
    fields:
      - name: location
        type:
          struct:
            - name: lat
              type: float64
            - name: lng
              type: float64
"#;

        // Act
        let file = load_yaml(yaml).unwrap();

        // Assert
        let field = &file.schemas[0].fields[0];
        assert_eq!(field.name, "location");
        match &field.arrow_type {
            ArrowType::Struct { fields } => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "lat");
                assert_eq!(fields[0].arrow_type, ArrowType::Float64);
                assert_eq!(fields[1].name, "lng");
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_struct_type_string() {
        // Arrange
        let input = "struct<name: utf8, qty: int32>";

        // Act
        let result = parse_type_string(input).unwrap();

        // Assert
        match result {
            ArrowType::Struct { fields } => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "name");
                assert_eq!(fields[0].arrow_type, ArrowType::Utf8);
                assert_eq!(fields[1].name, "qty");
                assert_eq!(fields[1].arrow_type, ArrowType::Int32);
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_list_of_struct() {
        // Arrange
        let input = "list<struct<name: utf8, qty: int32>>";

        // Act
        let result = parse_type_string(input).unwrap();

        // Assert
        match result {
            ArrowType::List { element } => match &element.arrow_type {
                ArrowType::Struct { fields } => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name, "name");
                    assert_eq!(fields[0].arrow_type, ArrowType::Utf8);
                    assert_eq!(fields[1].name, "qty");
                    assert_eq!(fields[1].arrow_type, ArrowType::Int32);
                }
                other => panic!("expected inner Struct, got {other:?}"),
            },
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_struct_with_nested_list() {
        // Arrange
        let input = "struct<name: utf8, scores: list<float64>>";

        // Act
        let result = parse_type_string(input).unwrap();

        // Assert
        match result {
            ArrowType::Struct { fields } => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "name");
                assert_eq!(fields[0].arrow_type, ArrowType::Utf8);
                assert_eq!(fields[1].name, "scores");
                match &fields[1].arrow_type {
                    ArrowType::List { element } => {
                        assert_eq!(element.arrow_type, ArrowType::Float64);
                    }
                    other => panic!("expected List, got {other:?}"),
                }
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_invalid_type_returns_error() {
        assert!(parse_type_string("").is_err());
        assert!(parse_type_string("foobar").is_err());
        assert!(parse_type_string("timestamp[invalid]").is_err());
    }
}

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_empty_field_name_in_struct_parses_but_validation_catches() {
        // Parser accepts empty field names — validation rejects them.
        let result = parse_type_string("struct<:int32>");
        assert!(result.is_ok()); // Parser allows it
                                 // (validate.rs rejects the empty name)
    }

    #[test]
    fn test_decimal_overflow_precision_rejected() {
        // Arrange & Act
        let result = parse_type_string("decimal128(256, 2)");

        // Assert — parser now rejects precision overflow
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of range"));
    }

    #[test]
    fn test_decimal_overflow_scale_rejected() {
        // Arrange & Act
        let result = parse_type_string("decimal128(10, 128)");

        // Assert — parser now rejects scale overflow
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of range"));
    }

    #[test]
    fn test_negative_fixed_size_list_rejected() {
        // Arrange & Act
        let result = parse_type_string("fixed_size_list<int32>(-1)");

        // Assert
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be positive"));
    }

    #[test]
    fn test_negative_fixed_size_binary_rejected() {
        // Arrange & Act
        let result = parse_type_string("fixed_size_binary(-256)");

        // Assert
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be positive"));
    }

    #[test]
    fn test_time32_with_microsecond_parses_but_validation_catches() {
        // Parser accepts any TimeUnit — validation checks Arrow constraints.
        let result = parse_type_string("time32[us]");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            ArrowType::Time32 {
                unit: TimeUnit::Microsecond
            }
        );
    }

    #[test]
    fn test_time64_with_second_parses_but_validation_catches() {
        let result = parse_type_string("time64[s]");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            ArrowType::Time64 {
                unit: TimeUnit::Second
            }
        );
    }

    #[test]
    fn test_decimal_scale_greater_than_precision_parses_but_validation_catches() {
        // Parser accepts any precision/scale in range — validation checks Arrow semantics.
        let result = parse_type_string("decimal128(2, 10)");
        assert!(result.is_ok());
        match result.unwrap() {
            ArrowType::Decimal128 { precision, scale } => {
                assert_eq!(precision, 2);
                assert_eq!(scale, 10);
            }
            other => panic!("expected Decimal128, got {other:?}"),
        }
    }

    #[test]
    fn test_list_with_empty_inner_type() {
        assert!(parse_type_string("list<>").is_err());
    }

    #[test]
    fn test_map_with_one_arg() {
        assert!(parse_type_string("map<int32>").is_err());
    }

    #[test]
    fn test_struct_with_trailing_comma() {
        assert!(parse_type_string("struct<a:int32,>").is_err());
    }

    #[test]
    fn test_struct_field_without_colon() {
        assert!(parse_type_string("struct<name>").is_err());
    }
}
