use codegen_core::CodeWriter;
use heck::ToPascalCase;

use arrowc_schema::arrow_type::ArrowType;
use arrowc_schema::schema_def::{ArrowFieldDef, ArrowSchemaDef};

use super::type_mapping::rust_type_info;

/// Info about a generated companion struct.
#[derive(Debug, Clone)]
pub struct CompanionInfo {
    /// The Rust struct name (e.g. `UserEventsLocation`).
    pub struct_name: String,
    /// The fields of the companion struct.
    pub fields: Vec<ArrowFieldDef>,
    /// Whether the struct needs a lifetime parameter.
    pub needs_lifetime: bool,
}

impl CompanionInfo {
    /// The type expression for use in function signatures.
    /// Returns e.g. `UserEventsLocation<'_>` or `UserEventsLocation`.
    pub fn type_ref(&self) -> String {
        if self.needs_lifetime {
            format!("{}<'_>", self.struct_name)
        } else {
            self.struct_name.clone()
        }
    }
}

/// Collect all struct types in a schema that need companion structs.
/// Returns (field_name -> CompanionInfo) for each struct field found
/// at the top level or inside a list.
pub fn collect_companions(schema: &ArrowSchemaDef) -> Vec<(String, CompanionInfo)> {
    let mut companions = Vec::new();
    for field in &schema.fields {
        collect_field_companions(
            &schema.name,
            &field.name,
            &field.arrow_type,
            &mut companions,
        );
    }
    companions
}

fn collect_field_companions(
    schema_name: &str,
    field_name: &str,
    ty: &ArrowType,
    out: &mut Vec<(String, CompanionInfo)>,
) {
    match ty {
        ArrowType::Struct { fields } => {
            let struct_name = format!("{}{}", schema_name, field_name.to_pascal_case());
            let needs_lifetime = fields.iter().any(|f| field_needs_lifetime(&f.arrow_type));
            out.push((
                field_name.to_string(),
                CompanionInfo {
                    struct_name: struct_name.clone(),
                    fields: fields.clone(),
                    needs_lifetime,
                },
            ));
            // Recurse into nested structs within this struct
            for f in fields {
                collect_field_companions(&struct_name, &f.name, &f.arrow_type, out);
            }
        }
        ArrowType::List { element } | ArrowType::LargeList { element } => {
            // If the list element is a struct, generate a companion for it
            collect_field_companions(schema_name, field_name, &element.arrow_type, out);
        }
        _ => {}
    }
}

fn field_needs_lifetime(ty: &ArrowType) -> bool {
    match ty {
        ArrowType::Utf8 | ArrowType::LargeUtf8 => true,
        ArrowType::Binary | ArrowType::LargeBinary | ArrowType::FixedSizeBinary { .. } => true,
        ArrowType::List { element } => field_needs_lifetime(&element.arrow_type),
        ArrowType::Struct { fields } => fields.iter().any(|f| field_needs_lifetime(&f.arrow_type)),
        _ => false,
    }
}

/// Generate all companion structs for a schema.
pub fn generate_companions(w: &mut CodeWriter, schema: &ArrowSchemaDef) {
    let companions = collect_companions(schema);
    for (_field_name, info) in &companions {
        generate_one_companion(w, info);
        w.blank();
    }
}

fn generate_one_companion(w: &mut CodeWriter, info: &CompanionInfo) {
    let lifetime = if info.needs_lifetime { "<'a>" } else { "" };

    w.line(&format!(
        "/// Companion struct for the `{}` nested type.",
        info.struct_name
    ));
    w.block(&format!("pub struct {}{lifetime}", info.struct_name), |w| {
        for field in &info.fields {
            let field_name = super::rust_field_ident(&field.name);
            let rust_type = companion_field_type(field);
            w.line(&format!("pub {field_name}: {rust_type},"));
        }
    });
}

/// Get the Rust type for a companion struct field.
fn companion_field_type(field: &ArrowFieldDef) -> String {
    let info = rust_type_info(&field.arrow_type);
    let base = match &field.arrow_type {
        ArrowType::Utf8 | ArrowType::LargeUtf8 => "&'a str".into(),
        ArrowType::Binary | ArrowType::LargeBinary | ArrowType::FixedSizeBinary { .. } => {
            "&'a [u8]".into()
        }
        _ => info.append_type,
    };

    if field.nullable {
        format!("Option<{base}>")
    } else {
        base
    }
}
