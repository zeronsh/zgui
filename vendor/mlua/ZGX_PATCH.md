# zgx patch to mlua 0.12.2

Vendored from crates.io `mlua` 0.12.2 (MIT, license file unchanged) and
published as `zgx-mlua`, its library still `mlua`, which zgx-script depends
on by path and version (`mlua = { package = "zgx-mlua", … }`): it builds
from crates.io with these changes and no `[patch]` section. The
crate's tests, benchmarks, examples and docs are left out (and their
targets and dev-dependencies from `Cargo.toml`). Additions are marked
"zgx patch" in the source; nothing upstream is changed in behaviour.

## Why

Building a Luau interface (`ui.row { gap = 8, ui.text { … }, … }`) calls
into Rust once per element, and that host call was most of the build: in
`mount_bench` (20,000 rows of three nodes), 72% of the build phase. Of it,
about a third was mlua's handling of values rather than zgx's work:

- Every key and value a table walk sees (`Table::for_each`) becomes a
  `Value`; strings, tables, functions and userdata are referenced, each a
  slot on mlua's reference thread filled (`lua_xpush`) and later freed
  (`lua_replace`), though zgx reads most keys only for their identity.
- Every node the script gets back is a userdata made in a protected call
  (the VM has a memory limit): a `pcall` and two new C closures each,
  garbage the moment the call returns.

## What

1. **`Table::for_each_borrowed`** (`src/table.rs`, Luau only): as
   `for_each`, passing each key and value as a `StackValue`, borrowed from
   the stack for one callback. It gives a string's bytes (`as_bytes`,
   `as_str`), its identity (`to_pointer`), its type, and the value owned
   only when asked (`to_value`, `get::<V>()`; `get::<UserDataRef<T>>()`
   borrows userdata without a reference, as `from_stack` always has).
   The type and a string's bytes are read as the pair is pushed: a
   scoped function the callback calls Lua with runs in another stack
   frame, where the pair's index names another slot, and bytes borrowed
   from that slot could be collected while the borrow lasts.
2. **`Lua::create_userdata_relaxed`** (`src/state.rs`, `src/state/raw.rs`):
   as `create_userdata`, with the memory limit set aside for the one
   allocation (`MemoryState::relax_limit_with`, as mlua allocates its own
   small objects) instead of a protected call around it. The allocation can
   then fail only when the system is out of memory, which aborts either way,
   so no Lua error can unwind through the caller. The VM may end up one
   userdata over its limit; its next allocation under the limit meets it.

   A plain unprotected constructor would not do: a Lua error raised inside
   a Rust callback reaches mlua's `catch_unwind` as a foreign exception and
   aborts the process, so creating userdata must either be protected or
   unable to fail.

3. **`RawLua::get_userdata_type_id`** (`src/state/raw.rs`): inlined, its
   error (a wrong or destroyed userdata, or a value that is none) reported
   by a cold function apart (`userdata_type_error`), as before. Every
   userdata method a script calls (`state:get()`, a node's) checks its
   `self` here; the call and the formatting code around the check cost each
   call ~29 instructions (research B9). No check changed.

zgx-script uses both in `node.rs`: `fill_from` walks a node's properties
with `for_each_borrowed` (a key's identity finds its property; a colour
written out is parsed where it lies; a child node is read without a
reference), and every node is made with `create_userdata_relaxed`.

## Checked

`mount_bench` (`cargo run --release -p zgx-script --example mount_bench`,
`N=20000`), retired instructions on macOS, interleaved runs: the Luau build
phase 794 -> 644 M (-19%), allocations 8.0 -> 7.3 per node; build and mount
together 2,335 -> 2,182 M (-6.6%). `list_state_bench` and `anime_bench`
are unchanged.

zgx-script's tests run against it, among them the time budget
(`runaway_scripts_hit_their_time_budget`), collection near the memory limit
(`garbage_from_builtins_is_collected_before_the_memory_limit`), a script
making nodes until the limit stops it
(`a_script_keeping_nodes_meets_the_memory_limit`) and nodes a script keeps
across mounts (`nodes_a_script_keeps_mount_again_after_collections`), in
debug and release builds.

Both additions are public, self-contained API, written to be offered
upstream.

## A memory alarm

`set_memory_alarm(state, at, alarm)` (Luau only, `memory.rs`): once the VM
holds more than `at` bytes, its allocator calls `alarm` once; the alarm is
then cleared until set again. zgx-script collects garbage near a VM's
memory limit this way: the alarm sets Luau's interrupt (all it may do: it
runs inside an allocation), and the interrupt collects at its next
safepoint. Before, an interrupt hook ran at every loop back edge to check.

The allocator compares an allocation against the least of the limit and
the alarm's threshold (`check_at`), once, as it compared against the limit
alone; past it, it tells which was reached. With the alarm as a second
comparison of its own, `mount_bench` ran 0.35% more instructions.

Checked: `the_interrupt_is_set_by_the_memory_alarm` (zgx-script), and the
memory-limit tests listed above, in debug and release builds.

## Luau 744's bytecode loads (zgx-mlua 0.12.5)

mlua-sys 0.13.0 builds Luau from `luau0-src` 0.22, and 0.22.1 (Luau 744,
2026-10-24, since yanked) failed mlua 0.12.2's setup as a Lua state was
made: "Error configuring Luau: attempt to load a text chunk (mode is 'b')"
(#633). Luau 744's compiler writes bytecode version 13, and mlua-sys's
`luaL_isbytecode` takes versions up to 11 for bytecode: mlua compiles a
chunk, then loads it as binary, and mlua-sys read it as text. zgx-mlua
0.12.3 held `luau0-src` at 0.22.0 (Luau 740).

zgx-mlua checks for bytecode itself (`luau::is_bytecode`, by the versions
Luau 0.22 loads, 14 at most): `Chunk::detect_mode` asks it, and chunks
load through `luau::luaL_loadbufferenv`, mlua-sys's loader copied line for
line but for that check (`RawLua::load_chunk_inner` calls it): a load does
the work it did, and Luau 744's bytecode loads as bytecode. `luau0-src` is
any 0.22 again. Drop this once mlua-sys knows version 13 (`LBC_VERSION_MAX`
in its `src/luau/compat.rs`).

The library's own tests run (`cargo test --features luau --lib`) with
`static_assertions` as a dev-dependency, which the published package
leaves out.
