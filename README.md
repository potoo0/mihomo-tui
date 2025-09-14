## Features

- Cross-platform support (macOS, Windows, Linux)
- Intuitive keyboard only control

![demo](/docs/assets/demo.gif)

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

# Log file, Optional, write log only if exists
log-file: /tmp/mihomo-tui.log

# Log level(silent/trace/debug/info/warning/error), Optional, default is error.
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
