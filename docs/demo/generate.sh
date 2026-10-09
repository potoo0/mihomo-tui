#!/usr/bin/env bash
# Run from the repository root: docs/demo/generate.sh
# Recording output: docs/demo/demo.gif
# Optional environment variables:
#   DEMO_TUI_ONLY=1               Open the TUI for a manual check; skip recording.
#   MIHOMO_BIN=/path/to/mihomo    Use an existing mihomo executable.
#   TUI_BIN=/path/to/tui          Use an existing TUI executable; skip cargo build.
#   BETAMAX_BIN=/path/to/betamax  Use an existing Betamax executable.
# Optional tool: gifsicle reduces the recorded GIF size when installed.
# Theme: edit recording/demo.tape; list choices with target/demo-tools/betamax themes.
set -euo pipefail

# ==================== Paths and pinned downloads ====================
demo_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
recording_dir="$demo_dir/recording"
repo_root=$(cd -- "$demo_dir/../.." && pwd)
mihomo_url=https://github.com/MetaCubeX/mihomo/releases/download/v1.19.32/mihomo-linux-amd64-v1-v1.19.32.gz
betamax_url=https://github.com/joshka/betamax/releases/download/betamax-v0.1.21/betamax-0.1.21-x86_64-unknown-linux-gnu.tgz
sarasa_url=https://github.com/be5invis/Sarasa-Gothic/releases/download/v1.0.33/SarasaMonoSC-TTF-1.0.33.7z
tool_dir="$repo_root/target/demo-tools"
sarasa_dir="$tool_dir/sarasa"

for program in awk curl find gzip tar python3 ss; do
  command -v "$program" >/dev/null || { echo "Missing: $program" >&2; exit 1; }
done

# ==================== Download and unpack tools ====================
download_binary() {
  local url=$1 output=$2 archive="$tool_dir/${1##*/}"
  mkdir -p "$tool_dir"
  echo "Downloading ${url##*/}..."
  curl --fail --location --retry 2 --silent --show-error \
    "$url" --output "$archive"

  # Unpack the release asset according to its filename suffix.
  case "$archive" in
    *.tgz)
      echo "Extracting ${archive##*/} with tar..."
      tar -xf "$archive" -C "$tool_dir"
      ;;
    *.gz)
      echo "Decompressing ${archive##*/} with gzip..."
      gzip -d "$archive"
      mv "${archive%.gz}" "$output"
      ;;
  esac
  rm -f -- "$archive"
  chmod +x "$output"
  echo "Ready: $output"
}

if [[ -z ${MIHOMO_BIN:-} ]]; then
  MIHOMO_BIN="$tool_dir/mihomo"
  [[ -x $MIHOMO_BIN ]] || download_binary "$mihomo_url" "$MIHOMO_BIN"
fi
[[ -x $MIHOMO_BIN ]] || { echo "MIHOMO_BIN is not executable: $MIHOMO_BIN" >&2; exit 1; }

# ==================== Build the TUI ====================
if [[ -z ${TUI_BIN:-} ]]; then
  command -v cargo >/dev/null || { echo "Missing: cargo" >&2; exit 1; }
  (cd "$repo_root" && cargo build --release --locked)
  TUI_BIN="$repo_root/target/release/mihomo-tui"
fi
[[ -x $TUI_BIN ]] || { echo "TUI_BIN is not executable: $TUI_BIN" >&2; exit 1; }
export TUI_BIN

# ==================== Prepare Betamax and Sarasa font ====================
if [[ ${DEMO_TUI_ONLY:-0} != 1 ]]; then
  if [[ -z ${BETAMAX_BIN:-} ]]; then
    BETAMAX_BIN="$tool_dir/betamax"
    [[ -x $BETAMAX_BIN ]] || download_binary "$betamax_url" "$BETAMAX_BIN"
  fi
  [[ -x $BETAMAX_BIN ]] || { echo "BETAMAX_BIN is not executable: $BETAMAX_BIN" >&2; exit 1; }

  if [[ ! -f $sarasa_dir/.ready ]]; then
    command -v 7z >/dev/null || { echo "Missing: 7z" >&2; exit 1; }
    sarasa_archive="$tool_dir/${sarasa_url##*/}"
    mkdir -p "$sarasa_dir"
    echo "Downloading ${sarasa_url##*/}..."
    curl --fail --location --retry 2 --silent --show-error \
      "$sarasa_url" --output "$sarasa_archive"
    echo "Extracting ${sarasa_archive##*/} with 7z..."
    7z x -y "$sarasa_archive" "-o$sarasa_dir" >/dev/null
    if [[ -z $(find "$sarasa_dir" -type f -name 'SarasaMonoSC-Regular.ttf' -print -quit) ]]; then
      echo "Sarasa Mono SC font not found in $sarasa_archive" >&2
      exit 1
    fi
    rm -f -- "$sarasa_archive"
    touch "$sarasa_dir/.ready"
    echo "Ready: $sarasa_dir"
  fi
fi

# ==================== Check local ports ====================
# Refuse occupied ports so a pre-existing mihomo cannot become the recording source.
occupied_ports=$(ss -H -ltn | awk '$4 ~ /:(18080|17892|19093)$/ {print $4}')
if [[ -n $occupied_ports ]]; then
  echo "Demo ports already in use: $occupied_ports" >&2
  exit 1
fi

# ==================== Temporary files and process cleanup ====================
workdir=$(mktemp -d)
server_pid=
mihomo_pid=
traffic_pid=
logs_pid=
cleanup() {
  local status=$? messages
  if (( status != 0 )); then
    echo "Demo generation failed." >&2
    if [[ ${DEMO_TUI_ONLY:-0} != 1 && -f $repo_root/target/demo-start.state.json ]]; then
      echo "Startup screen: $repo_root/target/demo-start.png" >&2
      echo "Terminal text: $repo_root/target/demo-start.state.json" >&2
    fi
    for log in "$workdir/mihomo.log" "$workdir/server.log"; do
      if [[ -f $log ]]; then
        messages=$(awk 'tolower($0) ~ /(error|warn|panic|traceback|fatal|code [45][0-9][0-9])/' "$log" | tail -n 15)
        if [[ -n $messages ]]; then
          echo "$log:" >&2
          echo "$messages" >&2
        fi
      fi
    done
  fi
  for pid in "$traffic_pid" "$logs_pid" "$mihomo_pid" "$server_pid"; do
    [[ -z $pid ]] || kill "$pid" 2>/dev/null || true
  done
  for pid in "$traffic_pid" "$logs_pid" "$mihomo_pid" "$server_pid"; do
    [[ -z $pid ]] || wait "$pid" 2>/dev/null || true
  done
  rm -rf -- "$workdir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

export XDG_CACHE_HOME="$workdir/cache"
mkdir -p "$XDG_CACHE_HOME"

# ==================== Start the local fixture server ====================
python3 "$recording_dir/server.py" >"$workdir/server.log" 2>&1 &
server_pid=$!
for _ in {1..50}; do
  if curl --fail --silent --noproxy '*' \
    http://127.0.0.1:18080/proxy-provider.yaml --output /dev/null; then
    break
  fi
  kill -0 "$server_pid" 2>/dev/null || { cat "$workdir/server.log" >&2; exit 1; }
  sleep 0.1
done
curl --fail --silent --noproxy '*' \
  http://127.0.0.1:18080/proxy-provider.yaml --output /dev/null

# ==================== Start the isolated mihomo ====================
mkdir -p "$workdir/core"
"$MIHOMO_BIN" -d "$workdir/core" -f "$recording_dir/mihomo.yaml" \
  >"$workdir/mihomo.log" 2>&1 &
mihomo_pid=$!
for _ in {1..100}; do
  if curl --fail --silent --noproxy '*' http://127.0.0.1:19093/version --output /dev/null; then
    break
  fi
  kill -0 "$mihomo_pid" 2>/dev/null || { cat "$workdir/mihomo.log" >&2; exit 1; }
  sleep 0.1
done
curl --fail --silent --noproxy '*' http://127.0.0.1:19093/version --output /dev/null
# The API can become ready before the mixed proxy port and providers finish
# starting. Wait until a request succeeds through this mihomo instance.
proxy_ready=false
for _ in {1..100}; do
  if curl --fail --silent --max-time 3 --noproxy '' \
    --proxy http://127.0.0.1:17892 \
    http://127.0.0.1:18080/health --output /dev/null; then
    proxy_ready=true
    break
  fi
  kill -0 "$mihomo_pid" 2>/dev/null || { cat "$workdir/mihomo.log" >&2; exit 1; }
  sleep 0.1
done
if [[ $proxy_ready != true ]]; then
  echo "Mihomo proxy did not become ready at 127.0.0.1:17892" >&2
  exit 1
fi

# ==================== Generate local traffic and record ====================
traffic_loop() {
  local index pid
  local pids=()
  trap 'for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done; exit 0' TERM
  while true; do
    pids=()
    for index in 1 2 3; do
      curl --fail --silent --show-error --max-time 90 \
        --noproxy '' --proxy http://127.0.0.1:17892 \
        "http://127.0.0.1:18080/traffic/$index" --output /dev/null &
      pids+=("$!")
    done
    for pid in "${pids[@]}"; do wait "$pid" || true; done
  done
}
traffic_loop >"$workdir/traffic.log" 2>&1 &
traffic_pid=$!

# Mihomo's log stream contains only new events. Keep producing local requests
# after the Logs tab subscribes, while the longer requests remain visible.
log_loop() {
  local curl_pid=
  trap '[[ -z $curl_pid ]] || kill "$curl_pid" 2>/dev/null || true; exit 0' TERM
  while true; do
    curl --fail --silent --show-error --max-time 5 --noproxy '' \
      --proxy http://127.0.0.1:17892 \
      http://127.0.0.1:18080/rule-provider.yaml --output /dev/null &
    curl_pid=$!
    wait "$curl_pid" || true
    curl_pid=
    sleep 0.3
  done
}
log_loop >"$workdir/log-traffic.log" 2>&1 &
logs_pid=$!

if [[ ${DEMO_TUI_ONLY:-0} == 1 ]]; then
  "$recording_dir/run-tui.sh"
else
  # Betamax starts its recording shell outside this directory. Resolve the
  # wrapper through PATH so the tape does not depend on that shell's cwd.
  (cd "$recording_dir" && PATH="$recording_dir:$PATH" FONTCONFIG_FILE="$recording_dir/betamax-fonts.xml" "$BETAMAX_BIN" run demo.tape)

  # ==================== Optimize the recorded GIF ====================
  if command -v gifsicle >/dev/null 2>&1; then
    optimized_gif="$workdir/demo-optimized.gif"
    if gifsicle -O3 "$demo_dir/demo.gif" -o "$optimized_gif"; then
      original_bytes=$(stat -c %s "$demo_dir/demo.gif")
      optimized_bytes=$(stat -c %s "$optimized_gif")
      if (( optimized_bytes < original_bytes )); then
        mv -- "$optimized_gif" "$demo_dir/demo.gif"
        echo "Optimized demo.gif: $original_bytes -> $optimized_bytes bytes"
      else
        echo "Gifsicle did not reduce demo.gif; keeping the original" >&2
      fi
    else
      echo "Warning: gifsicle failed; keeping the original demo.gif" >&2
    fi
  else
    echo "Warning: gifsicle is not installed; keeping the unoptimized demo.gif" >&2
  fi
  echo "Wrote $demo_dir/demo.gif"
fi
