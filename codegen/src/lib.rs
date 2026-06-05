pub mod rust;

use arrowc_schema::schema_def::ArrowSchemaFile;

/// Generate Rust code for all schemas in the file.
/// Returns a list of (filename, content) pairs.
pub fn generate_rust(file: &ArrowSchemaFile) -> Vec<(String, String)> {
    file.schemas
        .iter()
        .map(|schema| {
            let filename = heck::AsSnakeCase(&schema.name).to_string() + ".rs";
            let content = rust::generate_rust_file(schema);
            (filename, content)
        })
        .collect()
}
