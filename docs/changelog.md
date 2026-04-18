# Changelog

## 2026-04-17

- Added a standalone helix test publisher at [examples/helix_publisher.rs](/Users/camwolff/Development/cpss/gs_visualizer/examples/helix_publisher.rs:1) that publishes `foxglove.FrameTransforms` FlatBuffers into TISM.
- Added vendored Foxglove FlatBuffer schemas under [schemas/foxglove/flatbuffer](/Users/camwolff/Development/cpss/gs_visualizer/schemas/foxglove/flatbuffer/FrameTransforms.fbs:1), generated Rust bindings under [schemas/generated/rust](/Users/camwolff/Development/cpss/gs_visualizer/schemas/generated/rust/FrameTransforms_generated.rs:1), and the generated binary schema [schemas/generated/bfbs/FrameTransforms.bfbs](/Users/camwolff/Development/cpss/gs_visualizer/schemas/generated/bfbs/FrameTransforms.bfbs:1).
- Added [config.helix.toml](/Users/camwolff/Development/cpss/gs_visualizer/config.helix.toml:1) for the helix end-to-end test path.
- Updated the bridge shutdown path in [src/runtime.rs](/Users/camwolff/Development/cpss/gs_visualizer/src/runtime.rs:24) so `Ctrl-C` does not block indefinitely on worker teardown.
- Updated the bridge TISM consumer logic in [src/runtime.rs](/Users/camwolff/Development/cpss/gs_visualizer/src/runtime.rs:110) to match the current `tism` lifecycle:
  - the bridge treats `NotFound` as “publisher not currently live”
  - the bridge logs connection/waiting transitions instead of warning on every retry
  - cached `last_payload` state is cleared when the publisher disappears so restarted publishers republish their initial frame
- Replaced direct use of `tism::dynamic::open()` in [src/source_tism.rs](/Users/camwolff/Development/cpss/gs_visualizer/src/source_tism.rs:1) with a local zombie-aware dynamic reader because the current `tism` dynamic API does not yet check `is_zombie` on open, unlike the typed API.
- Updated the local reader in [src/source_tism.rs](/Users/camwolff/Development/cpss/gs_visualizer/src/source_tism.rs:1) for `tism` major version `2`, which is required for the current shared-memory header layout and reconnection path to work.
- Updated [examples/helix_publisher.rs](/Users/camwolff/Development/cpss/gs_visualizer/examples/helix_publisher.rs:76) to shut down cleanly on `Ctrl-C` so `OwnedSharedMemory` drops and marks the channel zombie in the updated `tism` lifecycle.
- Corrected the helix example in [examples/helix_publisher.rs](/Users/camwolff/Development/cpss/gs_visualizer/examples/helix_publisher.rs:118) to publish the requested `base -> rocket` transform instead of `world -> rocket`.
