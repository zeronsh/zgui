//! A window on an X screen whose default depth is neither 24 nor 32 bits: 16
//! (some VNC servers, older setups) or 30 (deep colour). Mesa's Vulkan offers
//! no format to present to such a window, and GL presents a 32-bit image
//! into it as if it were its own, so it showed nothing, or garbage. Such a
//! window takes a TrueColor visual of 24 bits instead, or of 32 (the one
//! compositing adds), which screens of any depth offer and every driver
//! presents to.
use winit::{
    event_loop::ActiveEventLoop,
    raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
};
use x11rb::{
    connection::Connection,
    protocol::xproto::{Screen, VisualClass},
};

/// The visual a new window takes on `event_loop`'s X screen, where its
/// default is not one to draw to; none on Wayland, or where it is.
pub(crate) fn visual(event_loop: &ActiveEventLoop) -> Option<u32> {
    let handle = event_loop.display_handle().ok()?;
    if !matches!(
        handle.as_raw(),
        RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)
    ) {
        return None;
    }
    // The screen's depth does not change while the app runs.
    static VISUAL: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    *VISUAL.get_or_init(|| {
        let (connection, screen) = x11rb::connect(None).ok()?;
        pick(&connection.setup().roots[screen])
    })
}

/// The visual a window on `screen` takes in place of its default: a
/// TrueColor one of 24 bits, else of 32; none where the default is either.
fn pick(screen: &Screen) -> Option<u32> {
    if matches!(screen.root_depth, 24 | 32) {
        return None;
    }
    let true_color = |depth: u8| {
        screen
            .allowed_depths
            .iter()
            .filter(|allowed| allowed.depth == depth)
            .flat_map(|allowed| &allowed.visuals)
            .find(|visual| {
                visual.class == VisualClass::TRUE_COLOR
                    && (visual.red_mask, visual.green_mask, visual.blue_mask)
                        == (0xff0000, 0xff00, 0xff)
            })
            .map(|visual| visual.visual_id)
    };
    true_color(24).or_else(|| true_color(32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11rb::protocol::xproto::{Depth, Visualtype};

    fn visual(id: u32, class: VisualClass, masks: (u32, u32, u32)) -> Visualtype {
        Visualtype {
            visual_id: id,
            class,
            bits_per_rgb_value: 8,
            colormap_entries: 256,
            red_mask: masks.0,
            green_mask: masks.1,
            blue_mask: masks.2,
        }
    }
    fn screen(root_depth: u8, depths: Vec<(u8, Vec<Visualtype>)>) -> Screen {
        Screen {
            root_depth,
            allowed_depths: depths
                .into_iter()
                .map(|(depth, visuals)| Depth { depth, visuals })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_window_on_a_16_or_30_bit_screen_takes_a_24_or_32_bit_visual() {
        let rgb565 = (0xf800, 0x7e0, 0x1f);
        let rgb888 = (0xff0000, 0xff00, 0xff);
        let rgb101010 = (0x3ff00000, 0xffc00, 0x3ff);
        // Xvfb at 16 bits: its own visuals, and the 32-bit one compositing adds.
        let sixteen = screen(
            16,
            vec![
                (16, vec![visual(0x21, VisualClass::TRUE_COLOR, rgb565)]),
                (32, vec![visual(0x40, VisualClass::TRUE_COLOR, rgb888)]),
            ],
        );
        assert_eq!(pick(&sixteen), Some(0x40));
        // A 24-bit visual is taken before a 32-bit one, a DirectColor never.
        let thirty = screen(
            30,
            vec![
                (30, vec![visual(0x21, VisualClass::TRUE_COLOR, rgb101010)]),
                (24, vec![visual(0x30, VisualClass::DIRECT_COLOR, rgb888)]),
                (24, vec![visual(0x31, VisualClass::TRUE_COLOR, rgb888)]),
                (32, vec![visual(0x40, VisualClass::TRUE_COLOR, rgb888)]),
            ],
        );
        assert_eq!(pick(&thirty), Some(0x31));
        // At 24 or 32 bits the default stands.
        let usual = screen(
            24,
            vec![(24, vec![visual(0x21, VisualClass::TRUE_COLOR, rgb888)])],
        );
        assert_eq!(pick(&usual), None);
    }
}
