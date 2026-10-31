# Luau native code: measured and shelved

Luau can compile scripts to machine code (mlua's `luau-jit` feature). We measured it in October 2026 and left it off: it isn't free when unused, its wins are in pure Luau loops, and it adds a signing trap and about 600 KB. Reopen this only if a real app shows a Luau loop as its bottleneck.

## Instructions on the arm64 gate

Against a6c97e5d. Rust-only cells didn't change. The app cells are one load and a few interactions, so compiling at load dominates them.

| Cell | Every module native | `--!native`, generator made per VM | `--!native`, generator made lazily |
| --- | --- | --- | --- |
| app-sketch | +133.71% | +2.25% | +0.79% |
| app-chat | +45.36% | +1.77% | +0.38% |
| app-code | +20.56% | +1.11% | +0.64% |
| app-dashboard-sort | +8.08% | +0.34% | +0.33% |
| app-mail | +4.54% | +0.41% | +0.19% |
| app-dashboard | +3.74% | +0.27% | +0.26% |
| mount | +0.87% | +0.05% | +0.08% |
| script-update | +0.21% | +0.04% | +0.03% |
| list-state | -10.37% | +0.05% | -0.05% |

The lazy column's 0.35–0.38 M a cell isn't native code running. It's the Luau compiler's load-time work (`CostVisitor::model`, `calcLinesSpan`, `fillBaselineInfo`) and, on app-code, `luau_execute` (+0.21 M). Those functions change when Luau is built with codegen; which build setting does it wasn't found.

## What it gains

40,000 rows on a Mac, the median of 15 runs:

| Work | Interpreter | Native |
| --- | --- | --- |
| Arithmetic loop | 273 M, 10 ms | 75–84 M, 2.3–3.4 ms |
| Sort with a Luau comparator | 262 M, 23–25 ms | 240 M, 14 ms |
| Filter | 16.7 M, 0.9 ms | 13.9 M, 0.7 ms |

## Hot reload

- With every module native, a reload of the mail example goes from 224.9 M, 14.4 ms to 243 M, 17 ms, because each reload compiles again. The catalog goes from 53.4 M to 59.1 M.
- Code memory didn't grow: footprint stayed level with the interpreter's over 100 reloads of mail and 500 of the catalog. These called `ScriptHost::reload` on unchanged files, so edit-to-screen time in a live window wasn't measured.
- zgx-script's tests passed with every module native, hot reload, budgets and error lines among them.

## Traps

- **macOS signing:** Luau maps its code with `MAP_JIT`. A hardened-runtime binary without the `com.apple.security.cs.allow-jit` entitlement runs interpreted, with no crash and no message.
- **`--!native`:** Luau's own flag for compiling only such modules isn't reachable through its C API, `luau_codegen_compile`, so the measured variants checked the chunk's source text. Precompiled chunks have no text to check.
- **Not measured:** x86, a different backend.
