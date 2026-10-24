//! `zgx::env` reads a `ZGX_` variable under its name from before the rename
//! too, for one release. A process of its own: it sets the environment.

#[test]
fn a_variable_is_read_under_its_old_name_when_its_new_one_is_unset() {
    // SAFETY: This test is its binary's only one: no other thread reads the
    // environment.
    unsafe {
        std::env::set_var("ZGUI_ALIAS_ONLY_OLD", "old"); // rename-to-zgx: keep
        std::env::set_var("ZGX_ALIAS_BOTH", "new");
        std::env::set_var("ZGUI_ALIAS_BOTH", "old"); // rename-to-zgx: keep
    }
    assert_eq!(zgx::env::var("ZGX_ALIAS_ONLY_OLD").as_deref(), Ok("old"));
    assert_eq!(zgx::env::var("ZGX_ALIAS_BOTH").as_deref(), Ok("new"));
    assert!(zgx::env::var_os("ZGX_ALIAS_NEITHER").is_none());
    // Only `ZGX_` names have an old one.
    assert!(zgx::env::var_os("ALIAS_ONLY_OLD").is_none());
}
