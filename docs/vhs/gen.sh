#!/bin/bash
set -e

### setup
# ln -f $PRJ_DIR/docs/vhs/gen.sh gen.sh
# ln -f $PRJ_DIR/docs/vhs/demo.tape demo.tape
# cargo build --release --target x86_64-unknown-linux-musl
# ln -f $PRJ_DIR/target/x86_64-unknown-linux-musl/release/mihomo-tui mihomo-tui
# cp -p ~/.config/mihomo-tui/config.yaml .

### download Sarasa font if not exists
if [ ! -d "sarasa" ]; then
  wget https://github.com/be5invis/Sarasa-Gothic/releases/download/v1.0.33/SarasaMonoSC-TTF-1.0.33.7z
  7z x SarasaMonoSC-TTF-1.0.33.7z -osarasa
fi

### run vhs in container
CONTAINER_RUNTIME=$(command -v docker >/dev/null 2>&1 && echo docker || echo podman)

$CONTAINER_RUNTIME run --rm \
  --name vhs \
  --network host \
  -v $PWD:/vhs \
  -v $PWD/sarasa:/usr/share/fonts/truetype/sarasa \
  --entrypoint bash \
  ghcr.io/charmbracelet/vhs \
  -c "fc-cache -f && vhs demo.tape"

### publish
#docker run --rm --name vhs-pub -v $PWD:/vhs --entrypoint bash ghcr.io/charmbracelet/vhs -c "vhs publish out.gif"
