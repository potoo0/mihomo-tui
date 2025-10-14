## 0.2.0 - 2025-10-12

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
