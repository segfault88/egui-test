# egui-test

A small [egui](https://github.com/emilk/egui) example rendered with [wgpu](https://wgpu.rs), using
[eframe](https://crates.io/crates/eframe). The same app runs as a native program and in the browser
on WebGPU (falling back to WebGL2 where WebGPU is unavailable).

**Try it in your browser: <https://segfault88.github.io/egui-test/>**

## Run natively

```bash
cargo run
```

## Run in the browser

```bash
./build-web.sh --serve
```

This builds the WebAssembly bundle into `dist/` and serves it on <http://localhost:8080>.
Pass `--dev` for a faster, unoptimised build. The script installs a `wasm-bindgen` CLI matching
`Cargo.lock` into `.tools/` on first run.

## Deployment

Every push to `main` builds the web version and publishes it to GitHub Pages via
[.github/workflows/deploy-web.yml](.github/workflows/deploy-web.yml). In the repository settings,
**Pages → Source** must be set to **GitHub Actions**.

## Layout

- `src/app.rs` – app state and the egui controls window, shared by both targets
- `src/triangle.rs`, `src/triangle.wgsl` – custom wgpu rendering, drawn behind the UI
- `src/main.rs` – native and web entry points
- `index.html` – page hosting the canvas for the web build
