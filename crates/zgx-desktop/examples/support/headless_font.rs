//! The headless benches' font: the comparison scenes' font file, built in
//! (zgx_workload::FONT_BYTES). The scenes name it (`zgx_workload::FONT`);
//! without it each machine would shape their text in whatever fallback it
//! has, and instruction counts would differ by machine (macOS has no DejaVu
//! Sans) and move when the fallback did.
use zgx_desktop::FontData;

pub fn font() -> FontData {
    FontData::new(std::sync::Arc::<[u8]>::from(zgx_workload::FONT_BYTES)).expect("the bundled font")
}
