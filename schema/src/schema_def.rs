use std::collections::BTreeMap;

use crate::arrow_type::ArrowType;

/// A single field in an Arrow schema.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrowFieldDef {
    pub name: String,
    pub arrow_type: ArrowType,
    pub nullable: bool,
    pub metadata: BTreeMap<String, String>,
}

impl ArrowFieldDef {
    pub fn new(name: impl Into<String>, arrow_type: ArrowType) -> Self {
        Self {
            name: name.into(),
            arrow_type,
            nullable: true,
            metadata: BTreeMap::new(),
        }
    }

    pub fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }
}

/// A named Arrow schema definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrowSchemaDef {
    pub name: String,
    pub fields: Vec<ArrowFieldDef>,
    pub metadata: BTreeMap<String, String>,
}

/// Top-level schema file containing one or more schema definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrowSchemaFile {
    pub schemas: Vec<ArrowSchemaDef>,
}
