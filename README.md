# gs_visualizer

`gs_visualizer` serves live TISM-backed data to Foxglove over a local WebSocket server.

## Run the server

Build the crate to regenerate the FlatBuffers artifacts from the sibling `../flatbuffers/foxglove` schema directory, then start the server with a config file:

```bash
cargo build
cargo run -- config/helix.toml
```

If you want a local data source to test against, start the bundled helix publisher in a second terminal:

```bash
cargo run --example helix_publisher
```

## Configure the server

Use [`config/example.toml`](config/example.toml) as the template for your own channel list.

- `server` controls the Foxglove WebSocket bind address, port, and advertised server name.
- `bridge` controls the default publish rate and retry interval for reopening TISM sources.
- Each `[[channels]]` entry maps one TISM shared-memory address to one Foxglove topic.
- `schema_path` is resolved relative to the config file, so configs under `config/` should usually point to `../schemas/bfbs/...`.

The generated schema binaries and Rust bindings live under `schemas/` and are refreshed automatically on `cargo build`.
