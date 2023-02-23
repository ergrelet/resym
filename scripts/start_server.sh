#!/usr/bin/env bash

set -eu

PORT=8888

script_path=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
cd "$script_path/.."

# Starts a local web-server that serves the contents of the `resym/pkg/` folder,
# i.e. the web-version of `resym`.
echo "Starting server…"
echo "Serving at http://localhost:${PORT}"

# Requires Python 3
(cd resym/pkg/ && python3 -m http.server ${PORT} --bind 127.0.0.1)
