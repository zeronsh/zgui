//! A host going idle collects its scripts' garbage (`zgx::idle`): Luau's
//! collector runs only as scripts allocate, so nodes a script let go of
//! would otherwise wait for the scripts to run a while.
mod common;

use common::Harness;

#[test]
fn going_idle_collects_the_scripts_garbage() {
    let h = Harness::inline(
        "local made = {}\n\
         for i = 1, 2000 do made[i] = ui.text { \"row\" } end\n\
         made = nil\n\
         return ui.text { \"x\" }\n",
    );
    assert_eq!(h.error(), None);
    let before = h.host.script_heap();
    zgx::idle::trim(&h.ui.runtime);
    let after = h.host.script_heap();
    assert!(
        after < before * 3 / 4,
        "going idle freed the dropped nodes: {before} → {after} bytes"
    );
    // Idle stays idle: nothing more to free, nothing more done.
    zgx::idle::trim(&h.ui.runtime);
    assert!(h.host.script_heap() <= after);
}

#[test]
fn a_host_configured_after_it_is_made_still_collects_when_idle() {
    // A builder method makes the host again (`ScriptHost::configure`): its
    // idle trim must follow it.
    let h = Harness::new(|runtime| {
        zgx_script::ScriptHost::inline(
            runtime,
            "app",
            "local made = {}\n\
             for i = 1, 2000 do made[i] = ui.text { \"row\" } end\n\
             made = nil\n\
             return ui.text { \"x\" }\n",
        )
        .app_id("dev.zgx.idle")
        .image_cache(1 << 20)
    });
    assert_eq!(h.error(), None);
    let before = h.host.script_heap();
    zgx::idle::trim(&h.ui.runtime);
    assert!(h.host.script_heap() < before * 3 / 4);
}

#[test]
fn collect_garbage_frees_what_scripts_dropped() {
    let h = Harness::inline(
        "local made = {}\n\
         for i = 1, 2000 do made[i] = ui.text { \"row\" } end\n\
         made = nil\n\
         return ui.text { \"x\" }\n",
    );
    let before = h.host.script_heap();
    h.host.collect_garbage();
    assert!(h.host.script_heap() < before * 3 / 4);
}
