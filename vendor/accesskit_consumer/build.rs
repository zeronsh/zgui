// zgx patch: zgx-desktop's build script reads this
// (`DEP_ZGX_ACCESSKIT_CONSUMER_PATCHED`): with this consumer (and the
// adapters patched beside it) a field's error message goes to the platform
// as its own; with crates.io's, zgx-desktop says it in the description.
fn main() {
    println!("cargo:patched=1");
}
