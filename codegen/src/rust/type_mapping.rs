use arrowc_schema::arrow_type::{ArrowType, TimeUnit};
use arrowc_schema::schema_def::ArrowFieldDef;

/// Mapping from ArrowType to Rust code generation strings.
pub struct RustTypeInfo {
    /// The Rust type for append parameters (e.g. `i32`, `&str`, `&[u8]`).
    pub append_type: String,
    /// The Arrow builder type (e.g. `Int32Builder`, `StringBuilder`).
    pub builder_type: String,
    /// The Arrow array type (e.g. `Int32Array`, `StringArray`).
    pub array_type: String,
    /// The `AsArray` accessor method (e.g. `as_primitive`, `as_string`).
    pub accessor_method: String,
    /// Constructor expression for the builder (e.g. `Int32Builder::new()`).
    pub builder_new: String,
    /// Constructor with capacity (e.g. `Int32Builder::with_capacity(capacity)`).
    pub builder_with_capacity: String,
    /// The Arrow DataType expression (e.g. `DataType::Int32`).
    pub datatype_expr: String,
}

/// Get the Rust type mapping for an ArrowType.
pub fn rust_type_info(ty: &ArrowType) -> RustTypeInfo {
    match ty {
        ArrowType::Null => RustTypeInfo {
            append_type: "()".into(),
            builder_type: "NullBuilder".into(),
            array_type: "NullArray".into(),
            accessor_method: "as_any".into(),
            builder_new: "NullBuilder::new()".into(),
            builder_with_capacity: "NullBuilder::new()".into(),
            datatype_expr: "DataType::Null".into(),
        },
        ArrowType::Boolean => RustTypeInfo {
            append_type: "bool".into(),
            builder_type: "BooleanBuilder".into(),
            array_type: "BooleanArray".into(),
            accessor_method: "as_boolean".into(),
            builder_new: "BooleanBuilder::new()".into(),
            builder_with_capacity: "BooleanBuilder::with_capacity(capacity)".into(),
            datatype_expr: "DataType::Boolean".into(),
        },
        ArrowType::Int8 => scalar_info("i8", "Int8"),
        ArrowType::Int16 => scalar_info("i16", "Int16"),
        ArrowType::Int32 => scalar_info("i32", "Int32"),
        ArrowType::Int64 => scalar_info("i64", "Int64"),
        ArrowType::UInt8 => scalar_info("u8", "UInt8"),
        ArrowType::UInt16 => scalar_info("u16", "UInt16"),
        ArrowType::UInt32 => scalar_info("u32", "UInt32"),
        ArrowType::UInt64 => scalar_info("u64", "UInt64"),
        ArrowType::Float16 => RustTypeInfo {
            append_type: "half::f16".into(),
            builder_type: "Float16Builder".into(),
            array_type: "Float16Array".into(),
            accessor_method: "as_primitive".into(),
            builder_new: "Float16Builder::new()".into(),
            builder_with_capacity: "Float16Builder::with_capacity(capacity)".into(),
            datatype_expr: "DataType::Float16".into(),
        },
        ArrowType::Float32 => scalar_info("f32", "Float32"),
        ArrowType::Float64 => scalar_info("f64", "Float64"),
        ArrowType::Utf8 => RustTypeInfo {
            append_type: "&str".into(),
            builder_type: "StringBuilder".into(),
            array_type: "StringArray".into(),
            accessor_method: "as_string".into(),
            builder_new: "StringBuilder::new()".into(),
            builder_with_capacity: "StringBuilder::with_capacity(capacity, capacity * 32)".into(),
            datatype_expr: "DataType::Utf8".into(),
        },
        ArrowType::LargeUtf8 => RustTypeInfo {
            append_type: "&str".into(),
            builder_type: "LargeStringBuilder".into(),
            array_type: "LargeStringArray".into(),
            accessor_method: "as_string".into(),
            builder_new: "LargeStringBuilder::new()".into(),
            builder_with_capacity: "LargeStringBuilder::with_capacity(capacity, capacity * 32)"
                .into(),
            datatype_expr: "DataType::LargeUtf8".into(),
        },
        ArrowType::Binary => RustTypeInfo {
            append_type: "&[u8]".into(),
            builder_type: "BinaryBuilder".into(),
            array_type: "BinaryArray".into(),
            accessor_method: "as_binary".into(),
            builder_new: "BinaryBuilder::new()".into(),
            builder_with_capacity: "BinaryBuilder::with_capacity(capacity, capacity * 64)".into(),
            datatype_expr: "DataType::Binary".into(),
        },
        ArrowType::LargeBinary => RustTypeInfo {
            append_type: "&[u8]".into(),
            builder_type: "LargeBinaryBuilder".into(),
            array_type: "LargeBinaryArray".into(),
            accessor_method: "as_binary".into(),
            builder_new: "LargeBinaryBuilder::new()".into(),
            builder_with_capacity: "LargeBinaryBuilder::with_capacity(capacity, capacity * 64)"
                .into(),
            datatype_expr: "DataType::LargeBinary".into(),
        },
        ArrowType::FixedSizeBinary { byte_width } => RustTypeInfo {
            append_type: "&[u8]".into(),
            builder_type: "FixedSizeBinaryBuilder".to_string(),
            array_type: "FixedSizeBinaryArray".into(),
            accessor_method: "as_fixed_size_binary".into(),
            builder_new: format!("FixedSizeBinaryBuilder::new({byte_width})"),
            builder_with_capacity: format!(
                "FixedSizeBinaryBuilder::with_capacity(capacity, {byte_width})"
            ),
            datatype_expr: format!("DataType::FixedSizeBinary({byte_width})"),
        },
        ArrowType::Date32 => RustTypeInfo {
            append_type: "i32".into(),
            builder_type: "Date32Builder".into(),
            array_type: "Date32Array".into(),
            accessor_method: "as_primitive".into(),
            builder_new: "Date32Builder::new()".into(),
            builder_with_capacity: "Date32Builder::with_capacity(capacity)".into(),
            datatype_expr: "DataType::Date32".into(),
        },
        ArrowType::Date64 => RustTypeInfo {
            append_type: "i64".into(),
            builder_type: "Date64Builder".into(),
            array_type: "Date64Array".into(),
            accessor_method: "as_primitive".into(),
            builder_new: "Date64Builder::new()".into(),
            builder_with_capacity: "Date64Builder::with_capacity(capacity)".into(),
            datatype_expr: "DataType::Date64".into(),
        },
        ArrowType::Time32 { unit } => {
            let suffix = time_unit_suffix(unit);
            RustTypeInfo {
                append_type: "i32".into(),
                builder_type: format!("Time32{suffix}Builder"),
                array_type: format!("Time32{suffix}Array"),
                accessor_method: "as_primitive".into(),
                builder_new: format!("Time32{suffix}Builder::new()"),
                builder_with_capacity: format!("Time32{suffix}Builder::with_capacity(capacity)"),
                datatype_expr: format!("DataType::Time32(TimeUnit::{suffix})"),
            }
        }
        ArrowType::Time64 { unit } => {
            let suffix = time_unit_suffix(unit);
            RustTypeInfo {
                append_type: "i64".into(),
                builder_type: format!("Time64{suffix}Builder"),
                array_type: format!("Time64{suffix}Array"),
                accessor_method: "as_primitive".into(),
                builder_new: format!("Time64{suffix}Builder::new()"),
                builder_with_capacity: format!("Time64{suffix}Builder::with_capacity(capacity)"),
                datatype_expr: format!("DataType::Time64(TimeUnit::{suffix})"),
            }
        }
        ArrowType::Timestamp { unit, timezone } => {
            let suffix = time_unit_suffix(unit);
            let tz_expr = match timezone {
                Some(tz) => format!("Some(Arc::from(\"{tz}\"))"),
                None => "None".into(),
            };
            let tz_chain = match timezone {
                Some(tz) => format!(".with_timezone(\"{tz}\")"),
                None => String::new(),
            };
            RustTypeInfo {
                append_type: "i64".into(),
                builder_type: format!("Timestamp{suffix}Builder"),
                array_type: format!("Timestamp{suffix}Array"),
                accessor_method: "as_primitive".into(),
                builder_new: format!("Timestamp{suffix}Builder::new(){tz_chain}"),
                builder_with_capacity: format!(
                    "Timestamp{suffix}Builder::with_capacity(capacity){tz_chain}"
                ),
                datatype_expr: format!("DataType::Timestamp(TimeUnit::{suffix}, {tz_expr})"),
            }
        }
        ArrowType::Duration { unit } => {
            let suffix = time_unit_suffix(unit);
            RustTypeInfo {
                append_type: "i64".into(),
                builder_type: format!("Duration{suffix}Builder"),
                array_type: format!("Duration{suffix}Array"),
                accessor_method: "as_primitive".into(),
                builder_new: format!("Duration{suffix}Builder::new()"),
                builder_with_capacity: format!(
                    "Duration{suffix}Builder::with_capacity(capacity)"
                ),
                datatype_expr: format!("DataType::Duration(TimeUnit::{suffix})"),
            }
        }
        ArrowType::Interval { .. } => {
            // Deferred to post-MVP
            todo!("interval codegen")
        }
        ArrowType::Decimal128 { precision, scale } => RustTypeInfo {
            append_type: "i128".into(),
            builder_type: "Decimal128Builder".into(),
            array_type: "Decimal128Array".into(),
            accessor_method: "as_primitive".into(),
            builder_new: format!(
                "Decimal128Builder::new().with_precision_and_scale({precision}, {scale}).unwrap()"
            ),
            builder_with_capacity: format!(
                "Decimal128Builder::with_capacity(capacity).with_precision_and_scale({precision}, {scale}).unwrap()"
            ),
            datatype_expr: format!("DataType::Decimal128({precision}, {scale})"),
        },
        ArrowType::Decimal256 { precision, scale } => RustTypeInfo {
            append_type: "i256".into(),
            builder_type: "Decimal256Builder".into(),
            array_type: "Decimal256Array".into(),
            accessor_method: "as_primitive".into(),
            builder_new: format!(
                "Decimal256Builder::new().with_precision_and_scale({precision}, {scale}).unwrap()"
            ),
            builder_with_capacity: format!(
                "Decimal256Builder::with_capacity(capacity).with_precision_and_scale({precision}, {scale}).unwrap()"
            ),
            datatype_expr: format!("DataType::Decimal256({precision}, {scale})"),
        },
        ArrowType::List { element } => list_info(element),
        ArrowType::Struct { fields } => struct_info(fields),
        // Deferred to post-MVP
        ArrowType::LargeList { .. }
        | ArrowType::FixedSizeList { .. }
        | ArrowType::Map { .. }
        | ArrowType::Union { .. }
        | ArrowType::Dictionary { .. } => {
            todo!("codegen for {ty}")
        }
    }
}

/// Generate the DataType expression for a field (used in schema generation).
pub fn datatype_expr(ty: &ArrowType) -> String {
    rust_type_info(ty).datatype_expr
}

/// Generate the Field expression for schema generation.
pub fn field_expr(field: &ArrowFieldDef) -> String {
    let nullable = field.nullable;
    match &field.arrow_type {
        ArrowType::List { element } => {
            let inner_field = field_expr(element);
            format!(
                "Field::new(\"{}\", DataType::List(Arc::new({inner_field})), {nullable})",
                field.name
            )
        }
        ArrowType::LargeList { element } => {
            let inner_field = field_expr(element);
            format!(
                "Field::new(\"{}\", DataType::LargeList(Arc::new({inner_field})), {nullable})",
                field.name
            )
        }
        ArrowType::FixedSizeList { element, size } => {
            let inner_field = field_expr(element);
            format!(
                "Field::new(\"{}\", DataType::FixedSizeList(Arc::new({inner_field}), {size}), {nullable})",
                field.name
            )
        }
        ArrowType::Struct { fields: sub_fields } => {
            let field_exprs: Vec<String> = sub_fields.iter().map(field_expr).collect();
            format!(
                "Field::new(\"{}\", DataType::Struct(Fields::from(vec![{}])), {nullable})",
                field.name,
                field_exprs.join(", ")
            )
        }
        ArrowType::Map {
            key,
            value,
            keys_sorted,
        } => {
            let entries_field = format!(
                "Field::new(\"entries\", DataType::Struct(Fields::from(vec![{}, {}])), false)",
                field_expr(key),
                field_expr(value)
            );
            format!(
                "Field::new(\"{}\", DataType::Map(Arc::new({entries_field}), {keys_sorted}), {nullable})",
                field.name
            )
        }
        _ => {
            let dt = datatype_expr(&field.arrow_type);
            format!("Field::new(\"{}\", {dt}, {nullable})", field.name)
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn scalar_info(rust_type: &str, arrow_prefix: &str) -> RustTypeInfo {
    RustTypeInfo {
        append_type: rust_type.into(),
        builder_type: format!("{arrow_prefix}Builder"),
        array_type: format!("{arrow_prefix}Array"),
        accessor_method: "as_primitive".into(),
        builder_new: format!("{arrow_prefix}Builder::new()"),
        builder_with_capacity: format!("{arrow_prefix}Builder::with_capacity(capacity)"),
        datatype_expr: format!("DataType::{arrow_prefix}"),
    }
}

fn time_unit_suffix(unit: &TimeUnit) -> &'static str {
    match unit {
        TimeUnit::Second => "Second",
        TimeUnit::Millisecond => "Millisecond",
        TimeUnit::Microsecond => "Microsecond",
        TimeUnit::Nanosecond => "Nanosecond",
    }
}

fn list_info(element: &ArrowFieldDef) -> RustTypeInfo {
    let inner = rust_type_info(&element.arrow_type);
    RustTypeInfo {
        append_type: format!("&[{}]", inner.append_type),
        builder_type: format!("ListBuilder<{}>", inner.builder_type),
        array_type: "ListArray".into(),
        accessor_method: "as_list".into(),
        builder_new: format!("ListBuilder::new({})", inner.builder_new),
        builder_with_capacity: format!(
            "ListBuilder::with_capacity({}, capacity)",
            inner.builder_new
        ),
        datatype_expr: format!(
            "DataType::List(Arc::new(Field::new(\"item\", {}, {})))",
            inner.datatype_expr, element.nullable
        ),
    }
}

fn struct_info(fields: &[ArrowFieldDef]) -> RustTypeInfo {
    // For struct fields, we generate a StructBuilder with child builders
    let child_builders: Vec<String> = fields
        .iter()
        .map(|f| {
            let info = rust_type_info(&f.arrow_type);
            format!("Box::new({}) as Box<dyn ArrayBuilder>", info.builder_new)
        })
        .collect();

    let field_exprs: Vec<String> = fields.iter().map(field_expr).collect();

    RustTypeInfo {
        append_type: "/* struct */".into(),
        builder_type: "StructBuilder".into(),
        array_type: "StructArray".into(),
        accessor_method: "as_struct".into(),
        builder_new: format!(
            "StructBuilder::new(vec![{}], vec![{}])",
            field_exprs.join(", "),
            child_builders.join(", ")
        ),
        builder_with_capacity: format!(
            "StructBuilder::new(vec![{}], vec![{}])",
            field_exprs.join(", "),
            child_builders.join(", ")
        ),
        datatype_expr: format!(
            "DataType::Struct(Fields::from(vec![{}]))",
            field_exprs.join(", ")
        ),
    }
}
