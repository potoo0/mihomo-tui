#!/usr/bin/env bash
set -euo pipefail

recording_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$recording_dir/../../.." && pwd)
# The app checks GitHub for releases on startup. Keep that check local while
# allowing its HTTP API requests to reach the demo core on loopback.
export HTTP_PROXY=http://127.0.0.1:9 HTTPS_PROXY=http://127.0.0.1:9
export ALL_PROXY=http://127.0.0.1:9 NO_PROXY=127.0.0.1,localhost
export http_proxy=$HTTP_PROXY https_proxy=$HTTPS_PROXY all_proxy=$ALL_PROXY
export no_proxy=$NO_PROXY
exec "${TUI_BIN:-$repo_root/target/release/mihomo-tui}" \
  --config "$recording_dir/tui.yaml" --runtime-config false
