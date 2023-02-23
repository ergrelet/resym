#!/usr/bin/env bash
set -eu
script_path=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
cd "$script_path/.."

CRATE_NAME="resym"

# Setup build environment
./scripts/setup_web.sh

# Build resym-web
RUSTFLAGS='--cfg=web_sys_unstable_apis -C target-feature=+atomics,+bulk-memory,+mutable-globals' rustup run nightly-2022-12-12 wasm-pack build --target web resym --no-default-features -Z build-std=panic_abort,std

# Copy web resources next to the output
cp $CRATE_NAME/resources/web/* $CRATE_NAME/pkg/
cp $CRATE_NAME/resources/resym_96.ico $CRATE_NAME/pkg/favicon.ico

echo "Finished '${CRATE_NAME}/pkg/${CRATE_NAME}_bg.wasm'"

if [[ "${OPEN}" == true ]]; then
  if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux, ex: Fedora
    xdg-open http://localhost:8888/index.html
  elif [[ "$OSTYPE" == "msys" ]]; then
    # Windows
    start http://localhost:8888/index.html
  else
    # Darwin/MacOS, or something else
    open http://localhost:8888/index.html
  fi
fi

