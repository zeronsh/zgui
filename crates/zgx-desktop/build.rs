//! With `terminal-bundle`: the terminal example's interface, compiled into
//! it (`zgx_script::include_bundle!("terminal")`), as it ships.
fn main() {
    #[cfg(feature = "terminal-bundle")]
    zgx_script::build::bundle("examples/terminal", "terminal")
        .expect("bundle the terminal example's scripts");
}
