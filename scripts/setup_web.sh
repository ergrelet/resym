#!/usr/bin/env bash
set -eu
script_path=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
cd "$script_path/.."

# Pre-requisites:
rustup target add wasm32-unknown-unknown
rustup component add rust-src --toolchain nightly-2022-12-12
if ! command -v wasm-pack &> /dev/null
then
    cargo install wasm-pack
fi
