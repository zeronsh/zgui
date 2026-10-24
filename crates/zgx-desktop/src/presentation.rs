//! Repair only regions changed since a recycled window buffer was last presented.
use std::{collections::VecDeque, num::NonZeroU32};
use zgx::scene::Rect;
/// Which regions of each recycled window buffer are out of date, from the
/// damage of the frames presented since it was last drawn.
#[derive(Default)]
pub struct Presentation {
    history: VecDeque<Vec<Rect>>,
    dimensions: (usize, usize),
}
impl Presentation {
    /// `age` is softbuffer's buffer age. Unknown/old buffers are fully initialized.
    /// Return current-frame surface damage separately from buffer repair regions.
    pub fn copy(
        &mut self,
        source: &[u32],
        target: &mut [u32],
        dimensions: (usize, usize),
        age: u8,
        damage: &[Rect],
    ) -> Vec<softbuffer::Rect> {
        let (width, height) = dimensions;
        assert_eq!(source.len(), width * height);
        assert_eq!(source.len(), target.len());
        if self.dimensions != dimensions {
            self.history.clear();
            self.dimensions = dimensions;
        }
        if age == 0 || age as usize > self.history.len() + 1 {
            target.copy_from_slice(source);
        } else {
            for r in damage
                .iter()
                .chain(self.history.iter().take(age as usize - 1).flatten())
            {
                let (x0, y0, x1, y1) = region(*r, width, height);
                for y in y0..y1 {
                    let start = y * width + x0;
                    let end = y * width + x1;
                    target[start..end].copy_from_slice(&source[start..end]);
                }
            }
        }
        if self.history.len() == 4 {
            self.history.pop_back();
        }
        self.history.push_front(damage.to_vec());
        damage
            .iter()
            .filter_map(|r| {
                let (x0, y0, x1, y1) = region(*r, width, height);
                Some(softbuffer::Rect {
                    x: x0 as u32,
                    y: y0 as u32,
                    width: NonZeroU32::new((x1 - x0) as u32)?,
                    height: NonZeroU32::new((y1 - y0) as u32)?,
                })
            })
            .collect()
    }
}
fn region(r: Rect, w: usize, h: usize) -> (usize, usize, usize, usize) {
    (
        r.x.floor().clamp(0., w as f32) as usize,
        r.y.floor().clamp(0., h as f32) as usize,
        (r.x + r.width).ceil().clamp(0., w as f32) as usize,
        (r.y + r.height).ceil().clamp(0., h as f32) as usize,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repairs_recycled_buffers_using_their_age() {
        let mut p = Presentation::default();
        let mut source = vec![1; 16];
        let mut a = vec![0; 16];
        let mut b = vec![0; 16];
        p.copy(&source, &mut a, (4, 4), 0, &[Rect::new(0., 0., 4., 4.)]);
        source[0] = 2;
        p.copy(&source, &mut b, (4, 4), 0, &[Rect::new(0., 0., 1., 1.)]);
        source[15] = 3;
        p.copy(&source, &mut a, (4, 4), 2, &[Rect::new(3., 3., 1., 1.)]);
        assert_eq!(source, a);
        source[7] = 4;
        p.copy(&source, &mut b, (4, 4), 2, &[Rect::new(3., 1., 1., 1.)]);
        assert_eq!(source, b);
        source[8] = 5;
        p.copy(&source, &mut b, (4, 4), 1, &[Rect::new(0., 2., 1., 1.)]);
        assert_eq!(source, b);
    }
}
