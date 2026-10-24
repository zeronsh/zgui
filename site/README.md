# Performance page

Static Cloudflare Pages site. `data.json` is the accepted benchmark summary;
`data.csv` contains all trial metrics. `measurement.json` identifies the source,
protocol, acceptance and environment. Publish only these intended public assets,
not the private repository or raw host process logs.

Serve with `python3 -m http.server 8788 --directory site`.
Deploy with `wrangler pages deploy site --project-name zgui-performance --branch main`.

Live: https://zgui-performance.pages.dev

Measured revision: `10b90de10aada565dd598605f7e6902a4c83ce88`. Results live in `docs/results/latest-linux-2026-09-26-10b90de`. Rebuild public data using `python3 scripts/build_performance_site.py docs/results/latest-linux-2026-09-26-10b90de`. The builder verifies complete trial identity and recomputes summaries before publishing aggregate metrics. Raw host logs and source archives stay in the private repository.

The rejected fcfcc9a attempt remains archived in docs/results/latest-linux-2026-09-25-fcfcc9a. The new 10b90de series passed all 36 trials and replaces the previous failure notice.

## macOS

`site/macos` holds the same comparison measured on a Mac with the Metal GPU,
chosen with the page's platform switch (or `#macos`). Memory there is the
process's physical footprint, what Activity Monitor reports, which includes
GPU memory it owns on unified memory; resident size is recorded too. Run it on
an awake Mac with nothing covering the windows:

    python3 scripts/run_macos_comparison.py
    python3 scripts/build_macos_site.py docs/results/latest-macos-<date>-<commit>

The builder publishes only accepted runs and recomputes every aggregate.

## macOS heavy dashboard

`site/macos-heavy` is a separate dataset, selected with `#macos-heavy`. It
contains 24 accepted trials of zgx and GPUI on the heavy dashboard, not the
three-framework lab scene. QuickGUI's heavy adapter panicked at startup and
is excluded rather than represented by zero. The UI uses physical footprint
and derives its chart and table frameworks from the selected data.

Evidence: `docs/results/latest-macos-heavy-2026-09-27-92fd6a4`. The measurement
was made on dirty source based on parent `92fd6a4`; the dashboard and download
record preserve that distinction and the source-provenance limitation.
Do not label that parent as the clean measured revision. The lab and heavy
scenes remain separately selectable and should not be compared directly.
