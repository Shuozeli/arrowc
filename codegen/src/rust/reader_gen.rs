use codegen_core::CodeWriter;

use arrowc_schema::schema_def::ArrowSchemaDef;

use super::type_mapping::rust_type_info;

/// Generate the `{Name}Reader<'a>` struct with typed column accessors.
pub fn generate_reader(w: &mut CodeWriter, schema: &ArrowSchemaDef) {
    let name = &schema.name;

    w.line(&format!("/// Typed reader for {name} RecordBatches."));
    w.line("///");
    w.line("/// Provides typed column accessors that return the correct Arrow array");
    w.line("/// types without requiring manual downcasting.");
    w.block(&format!("pub struct {name}Reader<'a>"), |w| {
        w.line("batch: &'a RecordBatch,");
    });

    w.blank();

    w.block(&format!("impl<'a> {name}Reader<'a>"), |w| {
        // try_new()
        w.line("/// Create a reader for the given RecordBatch.");
        w.line("///");
        w.line(&format!(
            "/// Returns an error if the batch schema does not match {name}."
        ));
        w.block(
            "pub fn try_new(batch: &'a RecordBatch) -> Result<Self, ArrowError>",
            |w| {
                w.line(&format!("let expected = {name}Schema::schema();"));
                w.block(
                    "if batch.schema().fields() != expected.fields()",
                    |w| {
                        w.line(&format!(
                            "return Err(ArrowError::SchemaError(format!(\"expected {name} schema, got {{:?}}\", batch.schema())));"
                        ));
                    },
                );
                w.line("Ok(Self { batch })");
            },
        );

        // Column accessors
        for (i, field) in schema.fields.iter().enumerate() {
            w.blank();
            let info = rust_type_info(&field.arrow_type);
            let field_name = super::rust_field_ident(&field.name);
            let array_type = &info.array_type;
            let accessor = &info.accessor_method;

            w.line(&format!(
                "/// Access the {} column as a {array_type}.",
                field.name
            ));
            w.block(
                &format!("pub fn {field_name}(&self) -> &{array_type}"),
                |w| {
                    w.line(&format!("self.batch.column({i}).{accessor}()"));
                },
            );
        }

        w.blank();

        // num_rows()
        w.line("/// Number of rows in the batch.");
        w.block("pub fn num_rows(&self) -> usize", |w| {
            w.line("self.batch.num_rows()");
        });

        w.blank();

        // batch()
        w.line("/// Access the underlying RecordBatch.");
        w.block("pub fn batch(&self) -> &RecordBatch", |w| {
            w.line("self.batch");
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
    fn test_generate_reader_simple() {
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
        generate_reader(&mut w, &schema);

        // Assert
        let output = w.finish();
        assert!(output.contains("pub struct TestReader<'a>"));
        assert!(output.contains("pub fn try_new(batch: &'a RecordBatch)"));
        assert!(output.contains("pub fn id(&self) -> &Int64Array"));
        assert!(output.contains("self.batch.column(0).as_primitive()"));
        assert!(output.contains("pub fn name(&self) -> &StringArray"));
        assert!(output.contains("self.batch.column(1).as_string()"));
        assert!(output.contains("pub fn num_rows(&self) -> usize"));
    }

    #[test]
    fn test_reader_escapes_reserved_word_field() {
        // Arrange — field named "type" (Rust keyword)
        let schema = ArrowSchemaDef {
            name: "Test".into(),
            fields: vec![ArrowFieldDef::new("type", ArrowType::Int32).with_nullable(false)],
            metadata: BTreeMap::new(),
        };
        let mut w = CodeWriter::new();

        // Act
        generate_reader(&mut w, &schema);

        // Assert — "type" should become "type_" in accessor method name
        let output = w.finish();
        assert!(
            output.contains("pub fn type_(&self) -> &Int32Array"),
            "missing escaped accessor in:\n{output}"
        );
    }
}
