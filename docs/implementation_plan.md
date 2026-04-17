# TISM -> Foxglove Bridge: Implementation Plan

## 1) What the docs imply (important constraints)

### TISM (`../tism`)
- `tism::dynamic::open(name)` allows consuming shared memory with unknown data size and reading raw `Vec<u8>`.
- Reads are lock-based; locks should be held briefly.
- For this bridge, `dynamic` mode is the right fit because payloads are binary FlatBuffers and channel sizes may vary by address.

### Foxglove Rust SDK (`foxglove = 0.21.0`)
- Use `WebSocketServer` for live streaming.
- Use `ChannelBuilder::build_raw()` + `RawChannel::log(&[u8])` for pre-serialized binary data.
- For `message_encoding = "flatbuffer"`, schema is required for channel advertisement over WebSocket.
  - In SDK source, `flatbuffer` is explicitly marked schema-required.
  - If missing, the channel is ignored for advertisement to clients.
- `Schema` accepts raw bytes; SDK handles base64 encoding for websocket transport of binary schema encodings.

### FlatBuffers + MCAP/Foxglove schema requirements
- FlatBuffer payloads are not self-describing enough for Foxglove visualization without schema metadata.
- For Foxglove custom FlatBuffers:
  - `message_encoding = "flatbuffer"`
  - `schema.encoding = "flatbuffer"`
  - `schema.data` should be `.bfbs` (binary schema) generated from `.fbs` via `flatc --schema -b`.
  - `schema.name` should match the fully-qualified root type/message name.

## 2) Recommended threading model

Use a **multi-threaded / multi-task runtime**:
- One polling task per TISM address/channel.
- Shared Foxglove context/server.
- Independent per-channel timing.

Why:
- A blocking/slow lock on one TISM channel should not delay all other channels.
- Different publish rates are easier and cleaner with one interval task per channel.
- Foxglove channels are designed to be shared across threads (`Arc`-based channel objects).

Single-threaded can work for low data rates, but it risks head-of-line blocking and timing jitter across channels.

## 3) Library architecture

Target crate layout:

- `src/lib.rs`
  - Public API (`Bridge`, `BridgeConfig`, `run`, `run_blocking`)
- `src/config.rs`
  - TOML config structs + validation
- `src/schema.rs`
  - Schema loading (`.bfbs` bytes from disk) and schema object construction
- `src/source_tism.rs`
  - TISM channel opening/reading abstraction (`dynamic::open`)
- `src/publisher.rs`
  - Foxglove channel creation + raw message publish
- `src/runtime.rs`
  - Task orchestration, shutdown, retries, and periodic scheduling
- `src/error.rs`
  - Unified error enum (`thiserror`)

## 4) TOML configuration design

```toml
[server]
host = "127.0.0.1"
port = 8765
name = "tism-bridge"
message_backlog_size = 1024

[bridge]
default_publish_hz = 30.0
open_retry_ms = 250

[[channels]]
tism_address = "imu_shm"
topic = "/imu"
publish_hz = 100.0
on_change_only = false
message_encoding = "flatbuffer"
schema_name = "foxglove.Imu"          # or custom namespace type
schema_encoding = "flatbuffer"        # default
schema_path = "schemas/Imu.bfbs"
max_message_bytes = 1048576

[channels.metadata]
frame = "base_link"
source = "tism"
```

Validation rules:
- `publish_hz > 0`
- `topic` unique
- `tism_address` unique
- if `message_encoding == "flatbuffer"` then `schema_name` + `schema_path` required
- `schema_path` must exist and be readable

## 5) Runtime behavior

Startup:
1. Load + validate TOML config.
2. Start Foxglove websocket server.
3. For each configured channel:
   - Load schema bytes (`.bfbs`) and create `foxglove::Schema`.
   - Build `RawChannel` with topic, encoding, schema, metadata.
   - Open TISM dynamic shared memory handle.
4. Spawn polling task per channel.

Per-channel loop:
1. Wait on interval (`publish_hz`).
2. Read TISM bytes (`Vec<u8>`).
3. Apply optional guards:
   - `on_change_only` (byte-compare to last payload)
   - `max_message_bytes`
4. Publish bytes with `RawChannel::log`.
5. On read/open error: log, backoff, retry.

Shutdown:
- Cancel tasks, close server handle, return cleanly.

## 6) Public API proposal

```rust
pub struct BridgeConfig { /* parsed TOML */ }
pub struct Bridge { /* runtime state */ }

impl Bridge {
    pub fn from_config(cfg: BridgeConfig) -> Result<Self, BridgeError>;
    pub async fn run_until_shutdown(self) -> Result<(), BridgeError>;
    pub fn run_blocking(self) -> Result<(), BridgeError>;
}

pub fn load_config(path: impl AsRef<std::path::Path>) -> Result<BridgeConfig, BridgeError>;
```

## 7) Implementation milestones

1. **Scaffold**
   - Convert crate to library-first structure, add config/error modules.
2. **Config + Validation**
   - Parse TOML with `serde` + `toml`, enforce required fields.
3. **Minimal End-to-End**
   - One channel: open TISM, build raw Foxglove channel with schema, publish on interval.
4. **Multi-channel Scheduler**
   - Spawn per-channel tasks, independent rates, retries, cancellation.
5. **Hardening**
   - Structured logging, size limits, on-change option, graceful shutdown.
6. **Tests**
   - Unit tests for config validation and schema loading.
   - Integration test with temporary TISM allocations + bridge loop smoke test.

## 8) Dependencies to add

- `serde` + `serde_derive`
- `toml`
- `thiserror`
- `tracing` + `tracing-subscriber`
- `tokio` (full or selected features for runtime/time/sync)

Optional:
- `clap` (if we add a small binary entrypoint that loads config path)
- `sha2` or fast hash crate (if `on_change_only` should compare hash instead of full bytes)

## 9) Open questions before coding

- Should `on_change_only` default to `true` or `false`?
- Do you want a tiny CLI binary in this crate (`bridge --config path.toml`) in addition to the library API?
- Do you want support for non-FlatBuffer encodings in v1, or strictly FlatBuffers first?
