
## 0.2.1 - 2025-12-17

### Features

- implement horizontal scrolling for logs component

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
