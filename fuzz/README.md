# Fuzzing

Targets for what zgui reads from outside the app, run with
[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) on nightly. They are
not part of the workspace and CI does not run them.

| Target | Reads |
| --- | --- |
| `markdown` | Markdown, as `ui.markdown` shows messages and documents |
| `json_value` | JSON a script receives (`ui.fetch` replies, web view messages), and written back |
| `theme` | A theme file: JSON with comments and trailing commas |
| `image` | PNG and JPEG (`zgui_gpu::assets::decode_image`) |
| `svg` | SVG, or a raster where one was asked for, drawn at a size |
| `gif` | Animated GIF frames and disposal |

```sh
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run -O theme -- -max_total_time=60
```

A crash leaves its input in `artifacts/<target>/`; `cargo +nightly fuzz run
<target> artifacts/<target>/crash-…` replays it. Fix it in the engine with a
test there, as any bug.

Not here, read by hand instead: the drop and portal URI lists
(`wayland_data::decode_uris`, `portal_dialog::file_uri`) are Linux-only and
small; `zgui.json` and `ui.persist` files go through serde_json, whose depth
limit bounds them.
