use codegen_core::CodeWriter;

use arrowc_schema::schema_def::ArrowSchemaDef;

use super::type_mapping::field_expr;

/// Generate the `{Name}Schema` struct with `schema()` and `schema_ref()` methods.
pub fn generate_schema(w: &mut CodeWriter, schema: &ArrowSchemaDef) {
    let name = &schema.name;

    w.line(&format!("/// Schema definition for {name}."));
    w.line(&format!("pub struct {name}Schema;"));
    w.blank();

    w.block(&format!("impl {name}Schema"), |w| {
        // schema() -> Schema
        w.line(&format!("/// Returns the Arrow schema for {name}."));
        w.block("pub fn schema() -> Schema", |w| {
            w.line("Schema::new(vec![");
            w.indent();
            for field in &schema.fields {
                let expr = field_expr(field);
                w.line(&format!("{expr},"));
            }
            w.dedent();
            w.line("])");
        });

        w.blank();

        // schema_ref() -> Arc<Schema>
        w.line(&format!(
            "/// Returns the Arrow schema for {name} wrapped in an Arc."
        ));
        w.block("pub fn schema_ref() -> Arc<Schema>", |w| {
            w.line("Arc::new(Self::schema())");
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrowc_schema::arrow_type::ArrowType;
    use arrowc_schema::schema_def::ArrowFieldDef;
    use std::collections::BTreeMap;

    #[test]
    fn test_generate_schema_simple() {
        // Arrange
        let schema = ArrowSchemaDef {
            name: "Test".into(),
            fields: vec![
                ArrowFieldDef::new("id", ArrowType::Int64).with_nullable(false),
                ArrowFieldDef::new("name", ArrowType::Utf8),
            ],
            metadata: BTreeMap::new(),
        };
        let mut w = CodeWriter::new();

        // Act
        generate_schema(&mut w, &schema);

        // Assert
        let output = w.finish();
        assert!(output.contains("pub struct TestSchema;"));
        assert!(output.contains("pub fn schema() -> Schema"));
        assert!(output.contains("pub fn schema_ref() -> Arc<Schema>"));
        assert!(output.contains("Field::new(\"id\", DataType::Int64, false)"));
        assert!(output.contains("Field::new(\"name\", DataType::Utf8, true)"));
    }
}
