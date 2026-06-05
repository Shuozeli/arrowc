use std::fmt;

/// Time unit for temporal Arrow types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Second,
    Millisecond,
    Microsecond,
    Nanosecond,
}

impl fmt::Display for TimeUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeUnit::Second => write!(f, "s"),
            TimeUnit::Millisecond => write!(f, "ms"),
            TimeUnit::Microsecond => write!(f, "us"),
            TimeUnit::Nanosecond => write!(f, "ns"),
        }
    }
}

impl TimeUnit {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "s" | "second" | "Second" => Some(TimeUnit::Second),
            "ms" | "millisecond" | "Millisecond" => Some(TimeUnit::Millisecond),
            "us" | "microsecond" | "Microsecond" => Some(TimeUnit::Microsecond),
            "ns" | "nanosecond" | "Nanosecond" => Some(TimeUnit::Nanosecond),
            _ => None,
        }
    }
}

/// Interval unit for Arrow Interval types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalUnit {
    YearMonth,
    DayTime,
    MonthDayNano,
}

/// Union mode for Arrow Union types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnionMode {
    Sparse,
    Dense,
}

/// Arrow data type IR. Mirrors `arrow::datatypes::DataType` structurally
/// but lives in the schema layer without depending on the arrow crate.
#[derive(Debug, Clone, PartialEq)]
pub enum ArrowType {
    // Null
    Null,

    // Scalars
    Boolean,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float16,
    Float32,
    Float64,

    // String / Binary
    Utf8,
    LargeUtf8,
    Binary,
    LargeBinary,
    FixedSizeBinary {
        byte_width: i32,
    },

    // Temporal
    Date32,
    Date64,
    Time32 {
        unit: TimeUnit,
    },
    Time64 {
        unit: TimeUnit,
    },
    Timestamp {
        unit: TimeUnit,
        timezone: Option<String>,
    },
    Duration {
        unit: TimeUnit,
    },
    Interval {
        unit: IntervalUnit,
    },

    // Decimal
    Decimal128 {
        precision: u8,
        scale: i8,
    },
    Decimal256 {
        precision: u8,
        scale: i8,
    },

    // Nested
    List {
        element: Box<super::schema_def::ArrowFieldDef>,
    },
    LargeList {
        element: Box<super::schema_def::ArrowFieldDef>,
    },
    FixedSizeList {
        element: Box<super::schema_def::ArrowFieldDef>,
        size: i32,
    },
    Struct {
        fields: Vec<super::schema_def::ArrowFieldDef>,
    },
    Map {
        key: Box<super::schema_def::ArrowFieldDef>,
        value: Box<super::schema_def::ArrowFieldDef>,
        keys_sorted: bool,
    },
    Union {
        fields: Vec<super::schema_def::ArrowFieldDef>,
        mode: UnionMode,
    },

    // Dictionary
    Dictionary {
        index_type: Box<ArrowType>,
        value_type: Box<ArrowType>,
    },
}

impl fmt::Display for ArrowType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArrowType::Null => write!(f, "null"),
            ArrowType::Boolean => write!(f, "boolean"),
            ArrowType::Int8 => write!(f, "int8"),
            ArrowType::Int16 => write!(f, "int16"),
            ArrowType::Int32 => write!(f, "int32"),
            ArrowType::Int64 => write!(f, "int64"),
            ArrowType::UInt8 => write!(f, "uint8"),
            ArrowType::UInt16 => write!(f, "uint16"),
            ArrowType::UInt32 => write!(f, "uint32"),
            ArrowType::UInt64 => write!(f, "uint64"),
            ArrowType::Float16 => write!(f, "float16"),
            ArrowType::Float32 => write!(f, "float32"),
            ArrowType::Float64 => write!(f, "float64"),
            ArrowType::Utf8 => write!(f, "utf8"),
            ArrowType::LargeUtf8 => write!(f, "large_utf8"),
            ArrowType::Binary => write!(f, "binary"),
            ArrowType::LargeBinary => write!(f, "large_binary"),
            ArrowType::FixedSizeBinary { byte_width } => {
                write!(f, "fixed_size_binary({byte_width})")
            }
            ArrowType::Date32 => write!(f, "date32"),
            ArrowType::Date64 => write!(f, "date64"),
            ArrowType::Time32 { unit } => write!(f, "time32[{unit}]"),
            ArrowType::Time64 { unit } => write!(f, "time64[{unit}]"),
            ArrowType::Timestamp {
                unit,
                timezone: None,
            } => write!(f, "timestamp[{unit}]"),
            ArrowType::Timestamp {
                unit,
                timezone: Some(tz),
            } => write!(f, "timestamp[{unit}, {tz}]"),
            ArrowType::Duration { unit } => write!(f, "duration[{unit}]"),
            ArrowType::Interval { unit } => match unit {
                IntervalUnit::YearMonth => write!(f, "interval[year_month]"),
                IntervalUnit::DayTime => write!(f, "interval[day_time]"),
                IntervalUnit::MonthDayNano => write!(f, "interval[month_day_nano]"),
            },
            ArrowType::Decimal128 { precision, scale } => {
                write!(f, "decimal128({precision}, {scale})")
            }
            ArrowType::Decimal256 { precision, scale } => {
                write!(f, "decimal256({precision}, {scale})")
            }
            ArrowType::List { element } => write!(f, "list<{}>", element.arrow_type),
            ArrowType::LargeList { element } => {
                write!(f, "large_list<{}>", element.arrow_type)
            }
            ArrowType::FixedSizeList { element, size } => {
                write!(f, "fixed_size_list<{}>({})", element.arrow_type, size)
            }
            ArrowType::Struct { .. } => write!(f, "struct"),
            ArrowType::Map { key, value, .. } => {
                write!(f, "map<{}, {}>", key.arrow_type, value.arrow_type)
            }
            ArrowType::Union { .. } => write!(f, "union"),
            ArrowType::Dictionary {
                index_type,
                value_type,
            } => write!(f, "dictionary<{index_type}, {value_type}>"),
        }
    }
}
