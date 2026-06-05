FROM rust:1.86-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependencies: copy only manifests first
COPY Cargo.toml Cargo.toml
COPY schema/Cargo.toml schema/Cargo.toml
COPY codegen/Cargo.toml codegen/Cargo.toml
COPY compiler/Cargo.toml compiler/Cargo.toml
COPY integration-tests/Cargo.toml integration-tests/Cargo.toml

# Create dummy sources so cargo can resolve the workspace
RUN mkdir -p schema/src codegen/src/rust compiler/src integration-tests/src/generated integration-tests/tests \
    && echo "pub mod arrow_type; pub mod input; pub mod schema_def; pub mod validate; #[derive(Debug, thiserror::Error)] pub enum SchemaError { #[error(\"x\")] X(String) }" > schema/src/lib.rs \
    && touch schema/src/arrow_type.rs schema/src/input.rs schema/src/schema_def.rs schema/src/validate.rs \
    && echo "pub mod rust;" > codegen/src/lib.rs \
    && mkdir -p codegen/src/rust && touch codegen/src/rust/mod.rs \
    && echo "" > compiler/src/lib.rs \
    && echo "fn main() {}" > compiler/src/main.rs \
    && echo "pub mod generated;" > integration-tests/src/lib.rs \
    && touch integration-tests/src/generated/mod.rs \
    && echo "// placeholder" > integration-tests/tests/duckdb_roundtrip.rs

# Download and compile dependencies (cached layer)
RUN cargo build --release -p arrowc-integration-tests --tests 2>/dev/null || true

# Now copy real source
COPY . .

# Touch all source files to invalidate the build cache for our code
RUN find . -name "*.rs" -exec touch {} +

# Build and test
RUN cargo test -p arrowc-integration-tests --release -- --test-threads=1
