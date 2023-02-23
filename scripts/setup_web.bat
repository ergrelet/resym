rustup target add wasm32-unknown-unknown
rustup component add rust-src --toolchain nightly-2022-12-12
where /q wasm-pack
IF ERRORLEVEL 1 (
    cargo install wasm-pack
)
