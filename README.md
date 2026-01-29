
[![Built With Ratatui](https://ratatui.rs/built-with-ratatui/badge.svg)](https://ratatui.rs/)

## Features

- Cross-platform support (macOS, Windows, Linux)
- Intuitive keyboard only control
- Real-time traffic and memory monitoring
- Proxy and proxy group management with latency testing
- Connection tracking
- Rule viewer with filtering and toggleable disabled states (meta >= v1.19.19)
- Live log streaming
- Core configuration editor with JSON5 comments and integrated system actions (Reload, Restart, etc.)

[screenshots](./docs/screenshots)

![demo](https://vhs.charm.sh/vhs-1DLl5vOjH1NFAO6V4C4jZh.gif)
> The terminal font shown in demo GIFs is [Sarasa Gothic](https://github.com/be5invis/Sarasa-Gothic),
> licensed under the [SIL Open Font License 1.1](https://scripts.sil.org/OFL).

## Limitations

- The tool is designed only to interact with the [API](https://wiki.metacubex.one/api/). It does not manage any actual configuration files.
- The tool uses a ring buffer to store the [last 500 connections](/src/components/mod.rs#L31).

## Installation

### With Cargo (Linux, macOS, Windows)

Installation via cargo:

```shell
rustup update stable

git clone https://github.com/potoo0/mihomo-tui && cd mihomo-tui
cargo install --path . --locked

```

### From binaries (Linux, macOS, Windows)

1. Download the [latest release binary](https://github.com/potoo0/mihomo-tui/releases)
2. Set the `PATH` environment variable

## Usage

```
$ mihomo-tui -h
Usage: mihomo-tui [OPTIONS]

Options:
  -c, --config <CONFIG_FILE>
          Path to config file (default: /home/wsl/.config/mihomo-tui/config.yaml)
  -h, --help
          Print help
  -V, --version
          Print version
```

## Configuration

The default location of the file depends on your OS:

- Linux: `$HOME/.config/mihomo-tui/config.yaml`
- macOS: `$HOME/Library/Application Support/io.github.potoo0.mihomo-tui/config.yaml`
- Windows: `%APPDATA%/potoo0/mihomo-tui/config/config.yaml`

The following is a sample config.toml file:

```yaml
# Mihomo external controller URL, Required
mihomo-api: http://127.0.0.1:9093

# Mihomo external controller secret, Optional
#mihomo-secret:

# Path to mihomo config JSON schema file, Optional, default is builtin core-config.schema.json
#mihomo-config-schema:

# Log file, Optional, write log only if exists
log-file: /tmp/mihomo-tui.log

# Log level(silent/trace/debug/info/warning/error), Optional, default is error.
# Examples:
#   error
#   info,mihomo_tui=debug
#   info,mihomo_tui=trace,mihomo_tui::app=debug
log-level: error
```

## Acknowledgments

Big thanks to the following projects:

- [ratatui](https://github.com/ratatui/ratatui)
- [metacubexd](https://github.com/MetaCubeX/metacubexd) - ui design
- [yozefu](https://github.com/MAIF/yozefu) - application pattern inspiration
- [btop](https://github.com/aristocratos/btop) - keyboard inspiration

## Contribution

Contributions, issues and pull requests are welcome!

### Code style

```bash
cargo +nightly fmt-check
cargo clippy-strict
```
