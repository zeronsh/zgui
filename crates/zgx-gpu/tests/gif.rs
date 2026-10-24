use std::{borrow::Cow, time::Duration};
use zgx::animation::LoopCount;
fn fixture(repeat: Option<gif::Repeat>) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut writer = gif::Encoder::new(
            &mut bytes,
            3,
            1,
            &[0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255],
        )
        .unwrap();
        if let Some(repeat) = repeat {
            writer.set_repeat(repeat).unwrap();
        }
        let frames = [
            (0, 3, vec![1, 1, 1], gif::DisposalMethod::Keep, 2),
            (1, 1, vec![2], gif::DisposalMethod::Previous, 3),
            (2, 1, vec![3], gif::DisposalMethod::Background, 0),
            (0, 1, vec![2], gif::DisposalMethod::Keep, 1),
        ];
        for (left, width, buffer, dispose, delay) in frames {
            writer
                .write_frame(&gif::Frame {
                    left,
                    width,
                    height: 1,
                    dispose,
                    delay,
                    buffer: Cow::Owned(buffer),
                    ..Default::default()
                })
                .unwrap();
        }
    }
    bytes
}
#[test]
fn gif_offsets_disposal_timing_and_repeat_extension_are_preserved() {
    let data = zgx_gpu::assets::decode_gif(&fixture(None)).unwrap();
    assert_eq!(data.loop_count(), LoopCount::ONCE);
    let pixels: Vec<_> = data.frames().iter().map(|f| f.image.pixels()).collect();
    assert_eq!(pixels[0], &[255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255]);
    assert_eq!(pixels[1], &[255, 0, 0, 255, 0, 255, 0, 255, 255, 0, 0, 255]);
    assert_eq!(pixels[2], &[255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255]);
    assert_eq!(pixels[3], &[0, 255, 0, 255, 255, 0, 0, 255, 0, 0, 0, 0]);
    assert_eq!(data.frames()[2].duration, Duration::from_millis(100));
    assert_eq!(
        zgx_gpu::assets::decode_gif(&fixture(Some(gif::Repeat::Infinite)))
            .unwrap()
            .loop_count(),
        LoopCount::Infinite
    );
    assert_eq!(
        zgx_gpu::assets::decode_gif(&fixture(Some(gif::Repeat::Finite(1))))
            .unwrap()
            .loop_count(),
        LoopCount::Finite(std::num::NonZeroU32::new(2).unwrap())
    );
    assert!(zgx_gpu::assets::decode_gif(b"invalid").is_err());
}
