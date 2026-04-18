# Implementation Details

This page documents the internal structure of `gs_visualizer`. It is intended for maintainers and for anyone extending the server, schema set, or runtime behavior.

## Architecture

The crate has three main responsibilities:

1. Load and validate a TOML configuration file.
2. Open a Foxglove WebSocket server and advertise configured channels.
3. Poll TISM shared-memory allocations, then republish each payload on its matching Foxglove topic.

`Bridge::run_until_shutdown` is the public entry point. Internally it delegates to `runtime::run_bridge`, which prepares channels, starts the WebSocket server, and spawns one blocking worker per configured TISM source.

## Runtime Flow

Each channel worker repeatedly:

1. Opens the configured TISM shared-memory allocation.
2. Reads the current payload while holding the shared-memory read lock.
3. Optionally suppresses repeated payloads when `on_change_only = true`.
4. Enforces `max_message_bytes` before publishing.
5. Logs the message to the already-registered Foxglove raw channel.

Workers keep retrying while the publisher is absent. A Ctrl-C signal triggers a coordinated shutdown: the runtime flips a shared shutdown flag, detaches workers, and stops the WebSocket server.

## Configuration Model

Configuration is defined by `BridgeConfig`, `ServerConfig`, `BridgeOptions`, and `ChannelConfig`.

- Relative `schema_path` values are resolved against the directory containing the config file.
- FlatBuffer channels require both `schema_name` and `schema_path`.
- Topic names and TISM addresses must be unique inside one config file.
- Publish rates must be finite and strictly positive.

The intended workflow is to keep runnable configs in `config/` and reference generated schema binaries under `../schemas/bfbs/`.

## Schema Generation

The source `.fbs` files are not stored in this repository anymore. Canonical FlatBuffers schemas live in the sibling `../flatbuffers/foxglove` repository.

`build.rs` scans that directory on every Cargo build, then regenerates:

- `schemas/rust/*.rs` for Rust FlatBuffers bindings
- `schemas/bfbs/*.bfbs` for Foxglove channel advertisement

This keeps the checked-out runtime crate focused on consumed artifacts while the shared schema repository remains the single source of truth.

## Foxglove Schema Scope

The shared schema directory is intentionally biased toward common Foxglove data types that are useful for this project:

- Transform and pose messages for rigid body visualization
- Scene primitives used by the 3D panel
- Image and compressed video messages for camera data
- Camera calibration support for projecting image data in 3D-aware workflows

Niche schemas are intentionally excluded so the generated output stays easy to inspect and maintain.
