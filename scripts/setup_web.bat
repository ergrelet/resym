rustup target add wasm32-unknown-unknown
rustup component add rust-src --toolchain nightly-2024-02-24
where /q wasm-pack
IF ERRORLEVEL 1 (
    cargo install wasm-pack
)
