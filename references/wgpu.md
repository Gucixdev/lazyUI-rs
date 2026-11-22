# wgpu - what matters

Safe Rust graphics API over Vulkan/Metal/DX12/GL/WebGPU. Source: `wgpu-trunk.zip`. Read: README, `docs/`, `naga/`, `wgpu-hal`,
`examples/`, `tests/`.

## Game changers

1. **One API, every backend**, selected at run time (`WGPU_BACKEND`, `WGPU_ADAPTER_NAME`); also runs in browsers (WebGPU, WebGL).
2. **naga**: shader translator (WGSL/GLSL/SPIR-V -> MSL/HLSL/SPIR-V/WGSL) with validation - shaders are data checked at load.
3. **Layering `wgpu` -> `wgpu-core` (validation, resource tracking) -> `wgpu-hal`** - a safe layer and an escape hatch in one repo.
4. **Headless adapters and a big test harness** (`tests/`, `cts_runner`): GPU code verified without a window.
5. **Explicit MSRV and governance policy** documented per crate.

## Decision

Not used in v1 (terminal). Value for us:

| wgpu idea | in lazyui |
|---|---|
| shader = function over a point returning coverage + colour | already in lazytui (`lt_shade`); the Rust port gets `canvas::shade(rect, f)` after the first slice |
| backend chosen at run time | `Surface` picked by flag/env: `LAZYUI_SURFACE=tty|headless` |
| headless first-class | same as rune; the test-suite renders text, not pixels |
| a GUI surface later | winit window + wgpu quad per cell + bitmap font = "lazyui in a window" (config of rune's uber-quad is the reference) |
