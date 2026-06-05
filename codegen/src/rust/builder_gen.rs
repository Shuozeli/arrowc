use std::collections::HashMap;

use codegen_core::CodeWriter;

use arrowc_schema::arrow_type::ArrowType;
use arrowc_schema::schema_def::{ArrowFieldDef, ArrowSchemaDef};

use super::companion_gen::CompanionInfo;
use super::type_mapping::rust_type_info;

/// Generate the `{Name}Builder` struct with typed `append()` and `finish()`.
pub fn generate_builder(
    w: &mut CodeWriter,
    schema: &ArrowSchemaDef,
    companions: &HashMap<String, CompanionInfo>,
) {
    let name = &schema.name;

    // Struct definition with builder fields
    w.line(&format!("/// Typed builder for {name} RecordBatches."));
    w.block(&format!("pub struct {name}Builder"), |w| {
        for field in &schema.fields {
            let info = rust_type_info(&field.arrow_type);
            let field_name = super::rust_field_ident(&field.name);
            w.line(&format!("{field_name}: {},", info.builder_type));
        }
    });

    w.blank();

    w.block(&format!("impl {name}Builder"), |w| {
        // new()
        w.line("/// Create a new builder with default capacity.");
        w.block("pub fn new() -> Self", |w| {
            w.block("Self", |w| {
                for field in &schema.fields {
                    let info = rust_type_info(&field.arrow_type);
                    let field_name = super::rust_field_ident(&field.name);
                    w.line(&format!("{field_name}: {},", info.builder_new));
                }
            });
        });

        w.blank();

        // with_capacity()
        w.line("/// Create a new builder with the given capacity hint.");
        w.block("pub fn with_capacity(capacity: usize) -> Self", |w| {
            w.block("Self", |w| {
                for field in &schema.fields {
                    let info = rust_type_info(&field.arrow_type);
                    let field_name = super::rust_field_ident(&field.name);
                    w.line(&format!("{field_name}: {},", info.builder_with_capacity));
                }
            });
        });

        w.blank();

        // append()
        generate_append_method(w, schema, companions);

        w.blank();

        // finish()
        w.line("/// Finish building and return the RecordBatch.");
        w.block(
            "pub fn finish(&mut self) -> Result<RecordBatch, ArrowError>",
            |w| {
                w.line("let columns: Vec<ArrayRef> = vec![");
                w.indent();
                for field in &schema.fields {
                    let field_name = super::rust_field_ident(&field.name);
                    w.line(&format!("Arc::new(self.{field_name}.finish()),"));
                }
                w.dedent();
                w.line("];");
                w.line(&format!(
                    "RecordBatch::try_new({name}Schema::schema_ref(), columns)"
                ));
            },
        );

        w.blank();

        // len()
        w.line("/// Return the number of rows appended so far.");
        w.block("pub fn len(&self) -> usize", |w| {
            if let Some(first) = schema.fields.first() {
                let field_name = super::rust_field_ident(&first.name);
                w.line(&format!("self.{field_name}.len()"));
            } else {
                w.line("0");
            }
        });

        w.blank();

        // is_empty()
        w.line("/// Returns true if no rows have been appended.");
        w.block("pub fn is_empty(&self) -> bool", |w| {
            w.line("self.len() == 0");
        });
    });

    w.blank();

    // Default impl
    w.block(&format!("impl Default for {name}Builder"), |w| {
        w.block("fn default() -> Self", |w| {
            w.line("Self::new()");
        });
    });
}

fn generate_append_method(
    w: &mut CodeWriter,
    schema: &ArrowSchemaDef,
    companions: &HashMap<String, CompanionInfo>,
) {
    // Build parameter list
    let params: Vec<String> = schema
        .fields
        .iter()
        .map(|field| {
            let field_name = super::rust_field_ident(&field.name);
            let param_type = append_param_type(field, companions);
            format!("{field_name}: {param_type}")
        })
        .collect();

    let params_str = params.join(", ");

    w.line("/// Append a row to the builder.");
    w.block(&format!("pub fn append(&mut self, {params_str})"), |w| {
        for field in &schema.fields {
            generate_field_append(w, field, companions);
        }
    });
}

/// Determine the parameter type for append based on field type and nullability.
fn append_param_type(field: &ArrowFieldDef, companions: &HashMap<String, CompanionInfo>) -> String {
    // Check for companion struct (struct field or list<struct>)
    if let Some(info) = companions.get(&field.name) {
        let type_ref = info.type_ref();
        return match &field.arrow_type {
            ArrowType::List { .. } | ArrowType::LargeList { .. } => {
                if field.nullable {
                    format!("Option<&[{type_ref}]>")
                } else {
                    format!("&[{type_ref}]")
                }
            }
            ArrowType::Struct { .. } => {
                if field.nullable {
                    format!("Option<&{type_ref}>")
                } else {
                    format!("&{type_ref}")
                }
            }
            _ => type_ref,
        };
    }

    let info = rust_type_info(&field.arrow_type);
    let base = &info.append_type;

    if field.nullable {
        match &field.arrow_type {
            ArrowType::Utf8 | ArrowType::LargeUtf8 => format!("Option<{base}>"),
            ArrowType::Binary | ArrowType::LargeBinary | ArrowType::FixedSizeBinary { .. } => {
                format!("Option<{base}>")
            }
            ArrowType::List { .. } => format!("Option<{base}>"),
            _ => format!("Option<{base}>"),
        }
    } else {
        base.clone()
    }
}

/// Generate the append logic for a single field.
fn generate_field_append(
    w: &mut CodeWriter,
    field: &ArrowFieldDef,
    companions: &HashMap<String, CompanionInfo>,
) {
    let field_name = super::rust_field_ident(&field.name);

    // Check if this field has a companion struct
    if let Some(info) = companions.get(&field.name) {
        match &field.arrow_type {
            ArrowType::Struct { fields: sub_fields } => {
                generate_struct_append(w, &field_name, field.nullable, sub_fields, info);
                return;
            }
            ArrowType::List { element } => {
                if let ArrowType::Struct { fields: sub_fields } = &element.arrow_type {
                    generate_list_struct_append(w, &field_name, field.nullable, sub_fields, info);
                    return;
                }
            }
            _ => {}
        }
    }

    match &field.arrow_type {
        ArrowType::Utf8 | ArrowType::LargeUtf8 => {
            if field.nullable {
                w.block(&format!("match {field_name}"), |w| {
                    w.line(&format!("Some(v) => self.{field_name}.append_value(v),"));
                    w.line(&format!("None => self.{field_name}.append_null(),"));
                });
            } else {
                w.line(&format!("self.{field_name}.append_value({field_name});"));
            }
        }
        ArrowType::Binary | ArrowType::LargeBinary | ArrowType::FixedSizeBinary { .. } => {
            if field.nullable {
                w.block(&format!("match {field_name}"), |w| {
                    w.line(&format!("Some(v) => self.{field_name}.append_value(v),"));
                    w.line(&format!("None => self.{field_name}.append_null(),"));
                });
            } else {
                w.line(&format!("self.{field_name}.append_value({field_name});"));
            }
        }
        ArrowType::List { element } => {
            let inner_info = rust_type_info(&element.arrow_type);
            if field.nullable {
                w.block(&format!("match {field_name}"), |w| {
                    w.block("Some(values) =>", |w| {
                        generate_list_values_append(
                            w,
                            &field_name,
                            &element.arrow_type,
                            &inner_info.append_type,
                        );
                        w.line(&format!("self.{field_name}.append(true);"));
                    });
                    w.block("None =>", |w| {
                        w.line(&format!("self.{field_name}.append(false);"));
                    });
                });
            } else {
                generate_list_values_append(
                    w,
                    &field_name,
                    &element.arrow_type,
                    &inner_info.append_type,
                );
                w.line(&format!("self.{field_name}.append(true);"));
            }
        }
        _ => {
            // Scalar types
            if field.nullable {
                w.line(&format!("self.{field_name}.append_option({field_name});"));
            } else {
                w.line(&format!("self.{field_name}.append_value({field_name});"));
            }
        }
    }
}

/// Generate typed append for a struct field using companion struct.
fn generate_struct_append(
    w: &mut CodeWriter,
    field_name: &str,
    nullable: bool,
    sub_fields: &[ArrowFieldDef],
    _info: &CompanionInfo,
) {
    let var = if nullable { "s" } else { field_name };

    if nullable {
        w.block(&format!("match {field_name}"), |w| {
            w.block("Some(s) =>", |w| {
                emit_struct_field_appends(w, field_name, var, sub_fields);
                w.line(&format!("self.{field_name}.append(true);"));
            });
            w.block("None =>", |w| {
                emit_struct_null_appends(w, field_name, sub_fields);
                w.line(&format!("self.{field_name}.append(false);"));
            });
        });
    } else {
        emit_struct_field_appends(w, field_name, var, sub_fields);
        w.line(&format!("self.{field_name}.append(true);"));
    }
}

/// Generate typed append for a list<struct> field using companion struct.
fn generate_list_struct_append(
    w: &mut CodeWriter,
    field_name: &str,
    nullable: bool,
    sub_fields: &[ArrowFieldDef],
    _info: &CompanionInfo,
) {
    if nullable {
        w.block(&format!("match {field_name}"), |w| {
            w.block("Some(rows) =>", |w| {
                w.block("for row in rows", |w| {
                    emit_struct_field_appends_via_list(w, field_name, "row", sub_fields);
                    w.line(&format!("self.{field_name}.values().append(true);"));
                });
                w.line(&format!("self.{field_name}.append(true);"));
            });
            w.block("None =>", |w| {
                w.line(&format!("self.{field_name}.append(false);"));
            });
        });
    } else {
        w.block(&format!("for row in {field_name}"), |w| {
            emit_struct_field_appends_via_list(w, field_name, "row", sub_fields);
            w.line(&format!("self.{field_name}.values().append(true);"));
        });
        w.line(&format!("self.{field_name}.append(true);"));
    }
}

/// Emit field_builder appends for a direct struct field.
fn emit_struct_field_appends(
    w: &mut CodeWriter,
    builder_field: &str,
    var: &str,
    sub_fields: &[ArrowFieldDef],
) {
    for (i, sf) in sub_fields.iter().enumerate() {
        let sf_name = super::rust_field_ident(&sf.name);
        let builder_type = struct_field_builder_type(&sf.arrow_type);
        if sf.nullable {
            w.line(&format!(
                "self.{builder_field}.field_builder::<{builder_type}>({i}).unwrap().append_option({var}.{sf_name});"
            ));
        } else {
            w.line(&format!(
                "self.{builder_field}.field_builder::<{builder_type}>({i}).unwrap().append_value({var}.{sf_name});"
            ));
        }
    }
}

/// Emit field_builder null appends when the struct itself is null.
fn emit_struct_null_appends(w: &mut CodeWriter, builder_field: &str, sub_fields: &[ArrowFieldDef]) {
    for (i, sf) in sub_fields.iter().enumerate() {
        let builder_type = struct_field_builder_type(&sf.arrow_type);
        w.line(&format!(
            "self.{builder_field}.field_builder::<{builder_type}>({i}).unwrap().append_null();"
        ));
    }
}

/// Emit field_builder appends for a struct inside a list (accessed via .values()).
fn emit_struct_field_appends_via_list(
    w: &mut CodeWriter,
    list_field: &str,
    var: &str,
    sub_fields: &[ArrowFieldDef],
) {
    for (i, sf) in sub_fields.iter().enumerate() {
        let sf_name = super::rust_field_ident(&sf.name);
        let builder_type = struct_field_builder_type(&sf.arrow_type);
        if sf.nullable {
            w.line(&format!(
                "self.{list_field}.values().field_builder::<{builder_type}>({i}).unwrap().append_option({var}.{sf_name});"
            ));
        } else {
            w.line(&format!(
                "self.{list_field}.values().field_builder::<{builder_type}>({i}).unwrap().append_value({var}.{sf_name});"
            ));
        }
    }
}

/// Get the Arrow builder type name for use in field_builder::<T>() calls.
fn struct_field_builder_type(ty: &ArrowType) -> String {
    rust_type_info(ty).builder_type
}

fn generate_list_values_append(
    w: &mut CodeWriter,
    field_name: &str,
    element_type: &ArrowType,
    _append_type: &str,
) {
    match element_type {
        ArrowType::Utf8 | ArrowType::LargeUtf8 => {
            w.block("for v in values", |w| {
                w.line(&format!("self.{field_name}.values().append_value(v);"));
            });
        }
        ArrowType::Binary | ArrowType::LargeBinary | ArrowType::FixedSizeBinary { .. } => {
            w.block("for v in values", |w| {
                w.line(&format!("self.{field_name}.values().append_value(v);"));
            });
        }
        _ => {
            w.block("for v in values", |w| {
                w.line(&format!("self.{field_name}.values().append_value(*v);"));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_generate_builder_simple() {
        // Arrange
        let schema = ArrowSchemaDef {
            name: "Test".into(),
            fields: vec![
                ArrowFieldDef::new("id", ArrowType::Int64).with_nullable(false),
                ArrowFieldDef::new("name", ArrowType::Utf8),
            ],
            metadata: BTreeMap::new(),
        };
        let companions = HashMap::new();
        let mut w = CodeWriter::new();

        // Act
        generate_builder(&mut w, &schema, &companions);

        // Assert
        let output = w.finish();
        assert!(output.contains("pub struct TestBuilder"));
        assert!(output.contains("id: Int64Builder"));
        assert!(output.contains("name: StringBuilder"));
        assert!(output.contains("pub fn new() -> Self"));
        assert!(output.contains("pub fn append(&mut self, id: i64, name: Option<&str>)"));
        assert!(output.contains("pub fn finish(&mut self)"));
        assert!(output.contains("self.id.append_value(id)"));
        assert!(output.contains("self.name.append_value(v)"));
    }

    #[test]
    fn test_generate_builder_with_struct_companion() {
        // Arrange
        let struct_fields = vec![
            ArrowFieldDef::new("lat", ArrowType::Float64),
            ArrowFieldDef::new("lng", ArrowType::Float64),
        ];
        let schema = ArrowSchemaDef {
            name: "Test".into(),
            fields: vec![
                ArrowFieldDef::new("id", ArrowType::Int64).with_nullable(false),
                ArrowFieldDef::new(
                    "location",
                    ArrowType::Struct {
                        fields: struct_fields.clone(),
                    },
                ),
            ],
            metadata: BTreeMap::new(),
        };
        let mut companions = HashMap::new();
        companions.insert(
            "location".into(),
            CompanionInfo {
                struct_name: "TestLocation".into(),
                fields: struct_fields,
                needs_lifetime: false,
            },
        );
        let mut w = CodeWriter::new();

        // Act
        generate_builder(&mut w, &schema, &companions);

        // Assert
        let output = w.finish();
        assert!(
            output.contains("location: Option<&TestLocation>"),
            "got: {output}"
        );
        assert!(output.contains("field_builder::<Float64Builder>(0)"));
        assert!(output.contains("field_builder::<Float64Builder>(1)"));
    }

    #[test]
    fn test_generate_builder_with_list_struct() {
        // Arrange
        let struct_fields = vec![
            ArrowFieldDef::new("name", ArrowType::Utf8),
            ArrowFieldDef::new("qty", ArrowType::Int32),
        ];
        let element = ArrowFieldDef::new(
            "item",
            ArrowType::Struct {
                fields: struct_fields.clone(),
            },
        );
        let schema = ArrowSchemaDef {
            name: "Order".into(),
            fields: vec![ArrowFieldDef::new(
                "items",
                ArrowType::List {
                    element: Box::new(element),
                },
            )],
            metadata: BTreeMap::new(),
        };
        let mut companions = HashMap::new();
        companions.insert(
            "items".into(),
            CompanionInfo {
                struct_name: "OrderItems".into(),
                fields: struct_fields,
                needs_lifetime: true,
            },
        );
        let mut w = CodeWriter::new();

        // Act
        generate_builder(&mut w, &schema, &companions);

        // Assert
        let output = w.finish();
        assert!(
            output.contains("items: Option<&[OrderItems<'_>]>"),
            "got: {output}"
        );
        assert!(output.contains("for row in rows"));
        assert!(output.contains("values().field_builder::<StringBuilder>(0)"));
        assert!(output.contains("values().field_builder::<Int32Builder>(1)"));
    }

    #[test]
    fn test_builder_escapes_reserved_word_field() {
        // Arrange — field named "type" (Rust keyword)
        let schema = ArrowSchemaDef {
            name: "Test".into(),
            fields: vec![ArrowFieldDef::new("type", ArrowType::Int32).with_nullable(false)],
            metadata: BTreeMap::new(),
        };
        let companions = HashMap::new();
        let mut w = CodeWriter::new();

        // Act
        generate_builder(&mut w, &schema, &companions);

        // Assert — "type" should become "type_" in all Rust identifiers
        let output = w.finish();
        assert!(
            output.contains("type_: Int32Builder"),
            "missing escaped struct field in:\n{output}"
        );
        assert!(
            output.contains("type_: i32"),
            "missing escaped append param in:\n{output}"
        );
        assert!(
            output.contains("self.type_.append_value(type_)"),
            "missing escaped append call in:\n{output}"
        );
    }
}
