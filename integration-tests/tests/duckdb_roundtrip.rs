/// DuckDB roundtrip integration tests for arrowc-generated code.
///
/// Test plan:
///   1. Build RecordBatches with generated Builders (typed, compile-time safe)
///   2. Write to Parquet files (Arrow → Parquet serialization)
///   3. Load into DuckDB (Parquet → DuckDB columnar storage)
///   4. Run SQL queries and verify data integrity
///   5. Read back with generated Readers (verify typed accessors)
///
/// Data design:
///
///   Products (5 rows — diverse null patterns):
///   ┌────┬──────────────────┬─────────┬──────────┬───────────┬─────────────────────┬────────────────────────────────┬──────────────────────┐
///   │ id │ name             │ price   │ in_stock │ weight_kg │ created_at          │ tags                           │ dimensions           │
///   ├────┼──────────────────┼─────────┼──────────┼───────────┼─────────────────────┼────────────────────────────────┼──────────────────────┤
///   │  1 │ Laptop Pro 15    │ 1299.99 │ true     │       1.8 │ 2024-01-15 10:30:00 │ [electronics, computer]        │ {40, 25, 2}          │
///   │  2 │ Cloud Storage    │    9.99 │ true     │      NULL │ 2024-02-20 14:00:00 │ [digital, subscription]        │ NULL                 │
///   │  3 │ USB-C Cable      │   12.50 │ true     │      0.05 │ 2024-03-01 08:00:00 │ NULL                           │ {100, 2, 2}          │
///   │  4 │ Mystery Box      │    NULL │ false    │       2.5 │ NULL                │ NULL                           │ {30, 30, 30}         │
///   │  5 │ Wireless Mouse   │   29.99 │ true     │       0.1 │ 2024-04-10 16:45:00 │ [electronics, accessory, wifi] │ {12, 6, 4}           │
///   └────┴──────────────────┴─────────┴──────────┴───────────┴─────────────────────┴────────────────────────────────┴──────────────────────┘
///
///   Sales (3 rows — varying list lengths, null shipping/notes):
///   ┌─────────┬──────────┬────────────┬─────────────────────────────────────────┬───────────────────────────────┬──────────────┬───────────────────┐
///   │ sale_id │ customer │ sale_date  │ items                                   │ shipping_address              │ total_amount │ notes             │
///   ├─────────┼──────────┼────────────┼─────────────────────────────────────────┼───────────────────────────────┼──────────────┼───────────────────┤
///   │       1 │ Alice    │ 2024-06-01 │ [{1,1,1299.99}, {5,2,29.99}]            │ {123 Main, Portland, OR, 97201} │      1359.97 │ Priority shipping │
///   │       2 │ Bob      │ 2024-06-15 │ [{3,5,12.50}]                           │ {456 Oak, Seattle, WA, 98101}   │        62.50 │ NULL              │
///   │       3 │ Charlie  │ 2024-07-01 │ [{2,1,9.99}, {3,2,12.50}, {5,1,29.99}] │ NULL                            │        64.98 │ NULL              │
///   └─────────┴──────────┴────────────┴─────────────────────────────────────────┴───────────────────────────────┴──────────────┴───────────────────┘
use std::fs::File;

use arrow::array::{Array, AsArray};
use arrow::record_batch::RecordBatch;
use duckdb::Connection;
use parquet::arrow::ArrowWriter;
use tempfile::TempDir;

use arrowc_integration_tests::generated::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Microseconds since epoch for a given datetime.
fn us(year: i32, month: u32, day: u32, h: u32, m: u32, s: u32) -> i64 {
    let days = days_since_epoch(year, month, day);
    let secs = days as i64 * 86400 + h as i64 * 3600 + m as i64 * 60 + s as i64;
    secs * 1_000_000
}

/// Days since epoch (1970-01-01) — simplified, no leap second handling.
fn days_since_epoch(year: i32, month: u32, day: u32) -> i32 {
    // Rata Die algorithm adapted for Unix epoch
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i32 - 719468
}

/// Decimal128 value from a floating-point dollar amount with scale=2.
fn dec2(dollars: f64) -> i128 {
    (dollars * 100.0).round() as i128
}

// ---------------------------------------------------------------------------
// Data builders
// ---------------------------------------------------------------------------

fn build_products() -> RecordBatch {
    let mut b = ProductBuilder::new();

    // Row 1: physical product, all fields populated
    b.append(
        1,
        "Laptop Pro 15",
        Some(dec2(1299.99)),
        Some(true),
        Some(1.8),
        Some(us(2024, 1, 15, 10, 30, 0)),
        Some(&["electronics", "computer"]),
        Some(&ProductDimensions {
            length_cm: Some(40.0),
            width_cm: Some(25.0),
            height_cm: Some(2.0),
        }),
    );

    // Row 2: digital product — no weight, no dimensions
    b.append(
        2,
        "Cloud Storage Plan",
        Some(dec2(9.99)),
        Some(true),
        None,
        Some(us(2024, 2, 20, 14, 0, 0)),
        Some(&["digital", "subscription"]),
        None,
    );

    // Row 3: cheap cable — no tags
    b.append(
        3,
        "USB-C Cable",
        Some(dec2(12.50)),
        Some(true),
        Some(0.05),
        Some(us(2024, 3, 1, 8, 0, 0)),
        None,
        Some(&ProductDimensions {
            length_cm: Some(100.0),
            width_cm: Some(2.0),
            height_cm: Some(2.0),
        }),
    );

    // Row 4: mystery box — no price, not in stock, no timestamp, no tags
    b.append(
        4,
        "Mystery Box",
        None,
        Some(false),
        Some(2.5),
        None,
        None,
        Some(&ProductDimensions {
            length_cm: Some(30.0),
            width_cm: Some(30.0),
            height_cm: Some(30.0),
        }),
    );

    // Row 5: mouse — 3 tags, all fields present
    b.append(
        5,
        "Wireless Mouse",
        Some(dec2(29.99)),
        Some(true),
        Some(0.1),
        Some(us(2024, 4, 10, 16, 45, 0)),
        Some(&["electronics", "accessory", "wifi"]),
        Some(&ProductDimensions {
            length_cm: Some(12.0),
            width_cm: Some(6.0),
            height_cm: Some(4.0),
        }),
    );

    b.finish().expect("build products")
}

fn build_sales() -> RecordBatch {
    let mut b = SaleBuilder::new();

    // Sale 1: Alice buys laptop + 2 mice, has shipping and notes
    b.append(
        1,
        "Alice",
        Some(days_since_epoch(2024, 6, 1)),
        Some(&[
            SaleItems {
                product_id: Some(1),
                quantity: Some(1),
                unit_price: Some(1299.99),
            },
            SaleItems {
                product_id: Some(5),
                quantity: Some(2),
                unit_price: Some(29.99),
            },
        ]),
        Some(&SaleShippingAddress {
            street: Some("123 Main St"),
            city: Some("Portland"),
            state: Some("OR"),
            zip: Some("97201"),
        }),
        Some(dec2(1359.97)),
        Some("Priority shipping"),
    );

    // Sale 2: Bob buys 5 cables, has shipping, no notes
    b.append(
        2,
        "Bob",
        Some(days_since_epoch(2024, 6, 15)),
        Some(&[SaleItems {
            product_id: Some(3),
            quantity: Some(5),
            unit_price: Some(12.50),
        }]),
        Some(&SaleShippingAddress {
            street: Some("456 Oak Ave"),
            city: Some("Seattle"),
            state: Some("WA"),
            zip: Some("98101"),
        }),
        Some(dec2(62.50)),
        None,
    );

    // Sale 3: Charlie buys 3 items (digital), no shipping, no notes
    b.append(
        3,
        "Charlie",
        Some(days_since_epoch(2024, 7, 1)),
        Some(&[
            SaleItems {
                product_id: Some(2),
                quantity: Some(1),
                unit_price: Some(9.99),
            },
            SaleItems {
                product_id: Some(3),
                quantity: Some(2),
                unit_price: Some(12.50),
            },
            SaleItems {
                product_id: Some(5),
                quantity: Some(1),
                unit_price: Some(29.99),
            },
        ]),
        None,
        Some(dec2(64.98)),
        None,
    );

    b.finish().expect("build sales")
}

fn write_parquet(batch: &RecordBatch, path: &std::path::Path) {
    let file = File::create(path).unwrap();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), None).unwrap();
    writer.write(batch).unwrap();
    writer.close().unwrap();
}

// ===========================================================================
// Tests
// ===========================================================================

// ---------- Product: Builder → Reader round-trip ----------

#[test]
fn test_product_builder_reader_roundtrip() {
    // Arrange
    let batch = build_products();

    // Act
    let reader = ProductReader::try_new(&batch).unwrap();

    // Assert — row count
    assert_eq!(reader.num_rows(), 5);

    // Assert — non-nullable columns
    assert_eq!(reader.id().value(0), 1);
    assert_eq!(reader.id().value(4), 5);
    assert_eq!(reader.name().value(0), "Laptop Pro 15");
    assert_eq!(reader.name().value(3), "Mystery Box");

    // Assert — nullable scalar: price
    assert_eq!(reader.price().value(0), dec2(1299.99));
    assert!(reader.price().is_null(3)); // Mystery Box has no price

    // Assert — nullable boolean
    assert!(reader.in_stock().value(0));
    assert!(!reader.in_stock().value(3));

    // Assert — nullable float: weight_kg
    assert!(reader.weight_kg().is_null(1)); // Cloud Storage has no weight
    assert!((reader.weight_kg().value(0) - 1.8).abs() < 1e-10);

    // Assert — nullable timestamp
    assert!(reader.created_at().is_null(3)); // Mystery Box has no timestamp
    assert!(!reader.created_at().is_null(0));
}

#[test]
fn test_product_tags_list_column() {
    // Arrange
    let batch = build_products();
    let reader = ProductReader::try_new(&batch).unwrap();

    // Act — row 0 should have ["electronics", "computer"]
    let tags = reader.tags();
    let row0_tags = tags.value(0);
    let row0_values: &arrow::array::StringArray = row0_tags.as_string();

    // Assert
    assert_eq!(row0_values.len(), 2);
    assert_eq!(row0_values.value(0), "electronics");
    assert_eq!(row0_values.value(1), "computer");

    // Row 2 (USB-C Cable) has null tags
    assert!(tags.is_null(2));

    // Row 4 (Wireless Mouse) has 3 tags
    let row4_tags = tags.value(4);
    let row4_values: &arrow::array::StringArray = row4_tags.as_string();
    assert_eq!(row4_values.len(), 3);
    assert_eq!(row4_values.value(2), "wifi");
}

#[test]
fn test_product_struct_dimensions() {
    // Arrange
    let batch = build_products();
    let reader = ProductReader::try_new(&batch).unwrap();

    // Act
    let dims = reader.dimensions();

    // Assert — row 0 (Laptop): {40, 25, 2}
    let length: &arrow::array::Float64Array = dims.column(0).as_primitive();
    let width: &arrow::array::Float64Array = dims.column(1).as_primitive();
    let height: &arrow::array::Float64Array = dims.column(2).as_primitive();
    assert!((length.value(0) - 40.0).abs() < 1e-10);
    assert!((width.value(0) - 25.0).abs() < 1e-10);
    assert!((height.value(0) - 2.0).abs() < 1e-10);

    // Row 1 (Cloud Storage): dimensions is null
    assert!(dims.is_null(1));

    // Row 3 (Mystery Box): {30, 30, 30}
    assert!((length.value(3) - 30.0).abs() < 1e-10);
}

// ---------- Sale: Builder → Reader round-trip ----------

#[test]
fn test_sale_builder_reader_roundtrip() {
    // Arrange
    let batch = build_sales();

    // Act
    let reader = SaleReader::try_new(&batch).unwrap();

    // Assert — basics
    assert_eq!(reader.num_rows(), 3);
    assert_eq!(reader.sale_id().value(0), 1);
    assert_eq!(reader.customer_name().value(0), "Alice");
    assert_eq!(reader.customer_name().value(2), "Charlie");

    // Assert — notes: only Alice has notes
    assert_eq!(reader.notes().value(0), "Priority shipping");
    assert!(reader.notes().is_null(1));
    assert!(reader.notes().is_null(2));

    // Assert — total_amount
    assert_eq!(reader.total_amount().value(0), dec2(1359.97));
    assert_eq!(reader.total_amount().value(1), dec2(62.50));
    assert_eq!(reader.total_amount().value(2), dec2(64.98));
}

#[test]
fn test_sale_items_list_struct() {
    // Arrange
    let batch = build_sales();
    let reader = SaleReader::try_new(&batch).unwrap();

    // Act — Alice's items (2 items)
    let items = reader.items();
    let row0 = items.value(0);
    let row0_struct: &arrow::array::StructArray = row0.as_struct();

    // Assert — Alice's first item: product_id=1, quantity=1, unit_price=1299.99
    let product_ids: &arrow::array::Int64Array = row0_struct.column(0).as_primitive();
    let quantities: &arrow::array::Int32Array = row0_struct.column(1).as_primitive();
    let prices: &arrow::array::Float64Array = row0_struct.column(2).as_primitive();

    assert_eq!(product_ids.len(), 2);
    assert_eq!(product_ids.value(0), 1);
    assert_eq!(product_ids.value(1), 5);
    assert_eq!(quantities.value(0), 1);
    assert_eq!(quantities.value(1), 2);
    assert!((prices.value(0) - 1299.99).abs() < 1e-10);
    assert!((prices.value(1) - 29.99).abs() < 1e-10);

    // Charlie's items (3 items)
    let row2 = items.value(2);
    let row2_struct: &arrow::array::StructArray = row2.as_struct();
    let r2_pids: &arrow::array::Int64Array = row2_struct.column(0).as_primitive();
    assert_eq!(r2_pids.len(), 3);
    assert_eq!(r2_pids.value(0), 2);
    assert_eq!(r2_pids.value(1), 3);
    assert_eq!(r2_pids.value(2), 5);
}

#[test]
fn test_sale_shipping_address_struct() {
    // Arrange
    let batch = build_sales();
    let reader = SaleReader::try_new(&batch).unwrap();

    // Act
    let shipping = reader.shipping_address();

    // Assert — Alice's address
    let cities: &arrow::array::StringArray = shipping.column(1).as_string();
    assert_eq!(cities.value(0), "Portland");
    assert_eq!(cities.value(1), "Seattle");

    // Charlie has null shipping
    assert!(shipping.is_null(2));
}

// ---------- Product: DuckDB Parquet round-trip ----------

#[test]
fn test_product_duckdb_parquet_roundtrip() {
    // Arrange
    let batch = build_products();
    let tmp = TempDir::new().unwrap();
    let parquet_path = tmp.path().join("products.parquet");
    write_parquet(&batch, &parquet_path);

    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE products AS SELECT * FROM read_parquet('{}')",
        parquet_path.display()
    ))
    .unwrap();

    // Act & Assert — row count
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM products", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 5);

    // Act & Assert — find the product with NULL price
    let null_price_name: String = conn
        .query_row("SELECT name FROM products WHERE price IS NULL", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(null_price_name, "Mystery Box");

    // Act & Assert — find products with NULL weight (digital products)
    let null_weight_name: String = conn
        .query_row(
            "SELECT name FROM products WHERE weight_kg IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(null_weight_name, "Cloud Storage Plan");

    // Act & Assert — boolean filter
    let out_of_stock: String = conn
        .query_row(
            "SELECT name FROM products WHERE in_stock = false",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(out_of_stock, "Mystery Box");

    // Act & Assert — decimal sum of all non-null prices
    let price_sum: f64 = conn
        .query_row(
            "SELECT CAST(SUM(price) AS DOUBLE) FROM products WHERE price IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    // 1299.99 + 9.99 + 12.50 + 29.99 = 1352.47
    assert!((price_sum - 1352.47).abs() < 0.01);

    // Act & Assert — timestamp ordering
    let earliest_name: String = conn
        .query_row(
            "SELECT name FROM products WHERE created_at IS NOT NULL ORDER BY created_at ASC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(earliest_name, "Laptop Pro 15");
}

#[test]
fn test_product_duckdb_nested_struct_access() {
    // Arrange
    let batch = build_products();
    let tmp = TempDir::new().unwrap();
    let parquet_path = tmp.path().join("products.parquet");
    write_parquet(&batch, &parquet_path);

    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE products AS SELECT * FROM read_parquet('{}')",
        parquet_path.display()
    ))
    .unwrap();

    // Act & Assert — struct field access: dimensions.length_cm
    let laptop_length: f64 = conn
        .query_row(
            "SELECT dimensions.length_cm FROM products WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!((laptop_length - 40.0).abs() < 1e-10);

    // Act & Assert — NULL struct: Cloud Storage has no dimensions
    let cloud_dims: Option<f64> = conn
        .query_row(
            "SELECT dimensions.length_cm FROM products WHERE id = 2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(cloud_dims.is_none());

    // Act & Assert — count products with dimensions
    let with_dims: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM products WHERE dimensions IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(with_dims, 4);
}

#[test]
fn test_product_duckdb_list_unnest() {
    // Arrange
    let batch = build_products();
    let tmp = TempDir::new().unwrap();
    let parquet_path = tmp.path().join("products.parquet");
    write_parquet(&batch, &parquet_path);

    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE products AS SELECT * FROM read_parquet('{}')",
        parquet_path.display()
    ))
    .unwrap();

    // Act & Assert — count total tags across all products
    let total_tags: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (SELECT UNNEST(tags) AS tag FROM products)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    // Row 0: 2, Row 1: 2, Row 2: NULL, Row 3: NULL, Row 4: 3 → 7 total
    assert_eq!(total_tags, 7);

    // Act & Assert — find products tagged "digital"
    let digital_product: String = conn
        .query_row(
            "SELECT name FROM products WHERE list_contains(tags, 'digital')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(digital_product, "Cloud Storage Plan");
}

// ---------- Sale: DuckDB Parquet round-trip ----------

#[test]
fn test_sale_duckdb_parquet_roundtrip() {
    // Arrange
    let batch = build_sales();
    let tmp = TempDir::new().unwrap();
    let parquet_path = tmp.path().join("sales.parquet");
    write_parquet(&batch, &parquet_path);

    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE sales AS SELECT * FROM read_parquet('{}')",
        parquet_path.display()
    ))
    .unwrap();

    // Act & Assert — row count
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 3);

    // Act & Assert — total revenue
    let total_revenue: f64 = conn
        .query_row(
            "SELECT CAST(SUM(total_amount) AS DOUBLE) FROM sales",
            [],
            |row| row.get(0),
        )
        .unwrap();
    // 1359.97 + 62.50 + 64.98 = 1487.45
    assert!((total_revenue - 1487.45).abs() < 0.01);

    // Act & Assert — notes null count
    let null_notes: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sales WHERE notes IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(null_notes, 2);
}

#[test]
fn test_sale_duckdb_list_struct_access() {
    // Arrange
    let batch = build_sales();
    let tmp = TempDir::new().unwrap();
    let parquet_path = tmp.path().join("sales.parquet");
    write_parquet(&batch, &parquet_path);

    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE sales AS SELECT * FROM read_parquet('{}')",
        parquet_path.display()
    ))
    .unwrap();

    // Act & Assert — count total line items across all sales
    let total_items: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM (SELECT UNNEST(items) AS item FROM sales)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    // Sale 1: 2, Sale 2: 1, Sale 3: 3 → 6 total
    assert_eq!(total_items, 6);

    // Act & Assert — Alice's total quantity ordered
    let alice_qty: i64 = conn
        .query_row(
            "SELECT CAST(SUM(item.quantity) AS BIGINT) FROM (
                SELECT UNNEST(items) AS item FROM sales WHERE customer_name = 'Alice'
            )",
            [],
            |row| row.get(0),
        )
        .unwrap();
    // 1 laptop + 2 mice = 3
    assert_eq!(alice_qty, 3);
}

#[test]
fn test_sale_duckdb_shipping_struct() {
    // Arrange
    let batch = build_sales();
    let tmp = TempDir::new().unwrap();
    let parquet_path = tmp.path().join("sales.parquet");
    write_parquet(&batch, &parquet_path);

    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&format!(
        "CREATE TABLE sales AS SELECT * FROM read_parquet('{}')",
        parquet_path.display()
    ))
    .unwrap();

    // Act & Assert — Alice's shipping city
    let city: String = conn
        .query_row(
            "SELECT shipping_address.city FROM sales WHERE sale_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(city, "Portland");

    // Act & Assert — Charlie has null shipping
    let charlie_city: Option<String> = conn
        .query_row(
            "SELECT shipping_address.city FROM sales WHERE sale_id = 3",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(charlie_city.is_none());

    // Act & Assert — count sales shipped to WA
    let wa_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sales WHERE shipping_address.state = 'WA'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(wa_count, 1);
}

// ---------- Schema validation ----------

#[test]
fn test_reader_rejects_wrong_schema() {
    // Arrange — build a Product batch, try to read as Sale
    let products = build_products();

    // Act
    let result = SaleReader::try_new(&products);

    // Assert
    assert!(result.is_err());
}

// ---------- Edge case tests ----------

#[test]
fn test_product_empty_batch() {
    // Arrange
    let mut b = ProductBuilder::new();

    // Act
    let batch = b.finish().expect("empty batch should succeed");

    // Assert
    assert_eq!(batch.num_rows(), 0);
    let reader = ProductReader::try_new(&batch).unwrap();
    assert_eq!(reader.num_rows(), 0);
}

#[test]
fn test_product_empty_list_vs_null_list() {
    // Arrange
    let mut b = ProductBuilder::new();
    // Row 0: empty tags list (Some(&[]), not null)
    b.append(1, "A", None, None, None, None, Some(&[]), None);
    // Row 1: null tags list (None)
    b.append(2, "B", None, None, None, None, None, None);

    // Act
    let batch = b.finish().unwrap();
    let reader = ProductReader::try_new(&batch).unwrap();
    let tags = reader.tags();

    // Assert — empty list is NOT null, has 0 elements
    assert!(!tags.is_null(0));
    assert_eq!(tags.value(0).len(), 0);

    // Assert — null list IS null
    assert!(tags.is_null(1));
}

#[test]
fn test_product_unicode_strings() {
    // Arrange
    let mut b = ProductBuilder::new();
    b.append(
        1,
        "Caf\u{00e9}",
        None,
        None,
        None,
        None,
        Some(&["\u{1F4BB}", "\u{2764}\u{FE0F}"]),
        None,
    );
    b.append(
        2,
        "\u{4F60}\u{597D}\u{4E16}\u{754C}",
        None,
        None,
        None,
        None,
        None,
        None,
    );

    // Act
    let batch = b.finish().unwrap();
    let reader = ProductReader::try_new(&batch).unwrap();

    // Assert
    assert_eq!(reader.name().value(0), "Caf\u{00e9}");
    assert_eq!(reader.name().value(1), "\u{4F60}\u{597D}\u{4E16}\u{754C}");
    let row0_tags = reader.tags().value(0);
    let tag_vals: &arrow::array::StringArray = row0_tags.as_string();
    assert_eq!(tag_vals.value(0), "\u{1F4BB}");
}

#[test]
fn test_product_parquet_reader_roundtrip() {
    // Arrange
    let batch = build_products();
    let tmp = TempDir::new().unwrap();
    let parquet_path = tmp.path().join("products.parquet");
    write_parquet(&batch, &parquet_path);

    // Act — read back from Parquet with arrow reader
    let file = File::open(&parquet_path).unwrap();
    let parquet_reader =
        parquet::arrow::arrow_reader::ParquetRecordBatchReader::try_new(file, 1024).unwrap();
    let batches: Vec<RecordBatch> = parquet_reader.collect::<Result<Vec<_>, _>>().unwrap();

    // Assert — verify via typed Reader
    assert_eq!(batches.len(), 1);
    let read_batch = &batches[0];
    let product_reader = ProductReader::try_new(read_batch).unwrap();
    assert_eq!(product_reader.num_rows(), 5);
    assert_eq!(product_reader.id().value(0), 1);
    assert_eq!(product_reader.name().value(0), "Laptop Pro 15");
    assert_eq!(product_reader.name().value(3), "Mystery Box");
    assert!(product_reader.price().is_null(3));
    assert_eq!(product_reader.price().value(0), dec2(1299.99));
}
