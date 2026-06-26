
## 0.4.4 - 2026-06-27

### Features

- **Filter**
  - Add field-scoped filter expressions for filterable views, for example `type:tcp` or `host:'google`.
  - Show available filter fields as the filter input placeholder.
- **Layout**
  - Improve narrow terminal layout support.
  - Add compact shortcut hints and adjust table/header rendering for constrained widths.
- **DNS**
  - Add focused shortcut hints in the DNS query dialog.
  - Support horizontal scrolling for long answer data.

### Refactors

- **Logs**
  - Improve live log update efficiency.

## 0.4.3 - 2026-06-13

### Features

- **Connections**
  - Add **Connections Settings** for configuring visible columns and source IP aliases, with support for loading settings from the config file.
  - Add more optional columns, including `Type`, `Process`, and `ConnectTime`.
  - Add source IP aliases for connection display.
- **Config**
  - Persist runtime UI settings in a sidecar runtime configuration file.
  - Add `mihomo-repo` configuration for checking mihomo core releases.
  - Add DNS query dialog from the config view.
- **Updates**
  - Add GitHub release update markers in the header.
  - Add update flow for both `mihomo-tui` and mihomo core releases.
- **API**
  - Reconnect websocket streams automatically after disconnects.

### Bug Fixes

- **Connections**: Keep the leading max-width column visible when rendering narrow tables.
- **Android**: Pin `reqwest` to avoid a rustls certificate verifier panic.

### Refactors

- Introduce reusable table column definitions.
- Share `KeyOutcome` across key handlers.
- Compact navigation shortcut hints.

### Chores

- Add manual build workflow.
- Replace `serde_yaml_ng` with `yaml_serde`.
- Bump dependency versions.

## 0.4.2 - 2026-05-24

### Bug Fixes

- **Proxy Detail**: Use the group delay endpoint when testing nested proxy groups.

## 0.4.1 - 2026-05-23

### Features

- **Config**
  - Load default proxy settings from the config file
- **Connections**
  - Add `T` to batch terminate active connections in the current filtered view.
- **Proxy**
  - Add configurable automatic connection termination after switching a proxy.

### Refactors

- Rename streaming API helpers to make stream-based methods explicit.
- Show `s` / `S` sort shortcut variants in **Proxy Detail** and **Proxy Provider Detail**.

## 0.4.0 - 2026-05-17

### Features

- **Config**
  - Add configurable buffer sizes for **Overview**, **Connections**, and **Logs**.
  - Add optional initial sort settings for **Connections**, **Proxy Detail**, and **Proxy Provider Detail**.
- **Shortcuts**
  - **Proxy Detail** / **Proxy Provider Detail**:
    - `s` to switch sort by: none, latency, name
    - `S` to toggle the sort direction

### Chores

- Add Android `aarch64-linux-android` release target and enable manual release workflow runs.
- Make local API tests opt-in behind the `local-api-test` feature and add a CI test job.
- Replace `vergen-gix` with `vergen-gitcl` to reduce vulnerable dependencies.

## 0.3.4 - 2026-04-14

### Features

- **Shortcuts**:
  - Add `Ctrl+l` to immediately clear all idle components.
  - **Proxy Detail**:
    - Add `[` / `]` for hierarchical proxy group navigation.
    - Add `c` to focus the currently selected proxy.
- **API**: Improved error reporting by including the response body in API error messages.

### Bug Fixes

- **Proxies**: Fixed a bug where failing latency tests could cause an infinite "testing" status.
- **Proxy Provider**: Fixed vertical navigation issues.

### Refactors

- Renamed `OverlayComponent` to `MsgBoxComponent`.
- Renamed `SearchComponent` to `FilterComponent`.
- Introduced `store` modules for component state management.
- Switched to explicit `tokio-console` feature flag for console-subscriber logic.

### Chores

- Integrated `rust-cache` into the Clippy lint workflow.
- Refined `tokio` features to reduce compile times.
- Bump dependency versions.

## 0.3.3 - 2026-03-28

### Features

- **Filter**: Replace `fuzzy_matcher` with `nucleo_matcher` for improved matching functionality. Syntax:
  - `str`: fuzzy match for `str`
  - `^str`: match if the value starts with `str`
  - `str$`: match if the value ends with `str`
  - `^str$`: match exactly `str`
  - `'str`: match if the value contains substring `str`
  - `!<pattern>`: negate the match of `<pattern>`, examples: `!^str`, `!'str`

### Bug Fixes

- **Connections**: Replace `HashMap` with `IndexMap` for stable history order.

### Chores

- Bump dependency versions.

## 0.3.2 - 2026-03-20

### Bug Fixes

- Fix integer underflow in UI component rendering.

### Chores

- Update Rust dependencies to newer minor versions.

## 0.3.1 - 2026-01-29

### Features

- **Filter**: Add `Ctrl+y` shortcut to yank the last deleted word.

### Refactors

- Improve readability of RFC3339Nano datetime in UI rendering.
- Make log level resolution order explicit.
- Improve Help component layout for better clarity and spacing.

### Chores

- Upgrade **ratatui** to `0.30.0` and adapt codebase to API changes.
- Bump dependency versions.

## 0.3.0 - 2026-01-22

### Features

- **Rules**
  - Support toggling rule disabled state (requires Meta ≥ v1.19.19).
  - Display rule hit statistics (requires Meta ≥ v1.19.19).
- **Rule Providers**
- **Config**
  - Edit and apply basic core configuration.
  - Trigger core actions: reload config, restart, flush Fake-IP cache, flush DNS cache, and update GEO database.

### Bug Fixes

- Correct connection chain display order.

### Chores

- Replace `HashMap` with `IndexMap` to preserve ordering where required.
- Add Linux arm64 musl build to release pipeline.
- Enhance error handling with `Action::Error`.
- Improve error logging with more detailed context.
- Reorder shortcut key display for clearer navigation semantics.

## 0.2.2 - 2026-01-06

### Features

- Remember filter pattern across components
- Release for Linux x86_64 musl

## 0.2.1 - 2025-12-17

### Features

- Implement horizontal scrolling for logs component

### Chores

- Fail fast when API server is unavailable.
- Avoid extra view clone when rendering table.

## 0.2.0 - 2025-10-14

### Features

- **Proxy Management**
    - Added proxy list view with latency indicators.
    - Implemented health check and manual switching between proxies.
    - Added proxy provider management, supporting display, health check, and on-demand updates.
- **Connection Capture Mode**
    - Introduced capture mode that continuously records connection data, including those that have already closed.

### Refactors

- Unified **scrolling and navigation logic** across components for consistent behavior.
- Replaced `color_eyre` with **`anyhow`** for simpler error handling and smaller dependency footprint.

### Chores

- Cleaned up unused fields and error handling actions.
- Minor UI adjustments and documentation updates.
- Bump dependency versions.

## 0.1.0 - 2025-09-14

- First release.
