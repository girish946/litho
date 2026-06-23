# Changelog

All notable changes to the `litho` crate and binaries are documented here.

## Unreleased

### Added

- **`litho-tui`** — interactive terminal UI with flash/clone mode selection, device and file pickers, progress display, and responsive layout (minimum 60×24 terminal).
- **TUI privilege flow** — runtime root detection; `pkexec` re-launch with `--mode`, `--device`, and `--image` pre-filled (`--start` never passed by elevation).
- **TUI file logging** — default log path `~/.cache/litho/litho-tui.log`; `--log-file` and `--log-level` CLI options.
- **`OperationProgress` API** — structured progress events (`OperationPhase`, bytes, percentage, message) replacing string-based pub-sub.
- **`devices::device_size_bytes()`** — read device size from sysfs for accurate clone progress.

### Changed

- **Library progress** — single `FnMut(OperationProgress)` callback; removed `simple-pub-sub` / `mio` dependencies.
- **Clone progress** — percentage now derived from bytes written vs device size (was incorrectly `bytes / 100`).
- **CLI `main`** — synchronous; removed `--sockfile` / pub-sub integration.
- **TUI module layout** — split into `app`, `ui`, `layout`, `helpers`, `privilege`, `logging`, `launch`.
- **pkexec relaunch** — uses `exec()` with inherited stdio and preserved `TERM` / locale env vars to keep the controlling TTY.

### Fixed

- **CLI clone** — correct argument order (`device`, then `file`).
- **TUI terminal errors** — TTY checks, logged terminal init/shutdown failures, terminal recovery after failed elevation.

### Added (alignment pass — pre real TUI I/O)

- Stronger polkit detection (`find_polkit_auth_agent` + `pkexec` on PATH)
- TUI logging: `--log-file=-`, `LITHO_LOG_STDERR=1`, 5 MiB log rotation
- Device list refresh log when `--device` pre-fills launch
- Footer shortcut hints on tall terminals; `scripts/record-demo.sh` for P11
- Extra tests: clap launch parsing, clone-style progress %, layout hints

### Added (remaining work P1–P10)

- **TUI** — `tui/operation.rs` operation runner (simulated progress; real `liblitho` I/O disabled in TUI)
- **TUI** — focus **Start** when launch args pre-fill; polkit hint in header; richer startup logs
- **CLI** — `--json-progress` emits JSON `OperationProgress` lines on stdout
- **Tests** — layout and launch unit tests; `OperationProgress` derives `Serialize`

### Known limitations

- **TUI** — flash/clone are simulated; use the `litho` CLI or library for real I/O.
- **Device vendor** — NVMe drives often lack `/sys/block/.../device/vendor`; a warn-level log is expected.
- **Platform** — device enumeration and full E2E support are Linux-first; see `platform-support.md`.