
## 0.3.1 - 2026-01-29

### Features

- **Search**
  - Add `Ctrl+y` shortcut to yank the last deleted word.

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

- Remember search pattern across components
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
