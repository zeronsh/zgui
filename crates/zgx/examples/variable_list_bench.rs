//! Core index microbenchmark, not GUI frame, process RSS or framework rankings.
//! Run with `cargo run --release -p zgx --example variable_list_bench`.
use std::{hint::black_box, time::Instant};
use zgx::virtual_list::{HeightIndex, VirtualList};

const OPERATIONS: usize = 10_000;
const ESTIMATE: f32 = 28.;
const VIEWPORT: f32 = 400.;
const OVERSCAN: usize = 2;

fn offset(index: usize, total: f32) -> f32 {
    // Identical viewport positions for the equal-height fixed/variable scan.
    ((index as u64 * 104_729 % 10_000) as f64 / 10_000. * f64::from((total - VIEWPORT).max(0.)))
        as f32
}
fn main() {
    for count in [100_000, 1_000_000] {
        let started = Instant::now();
        let mut heights = HeightIndex::new(black_box(count), black_box(ESTIMATE));
        let construction_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let fixed = VirtualList::new(black_box(count), black_box(ESTIMATE), OVERSCAN);
        let fixed_construction_ns = started.elapsed().as_nanos();
        assert_eq!(heights.content_height(), fixed.content_height());

        let started = Instant::now();
        let mut variable_checksum = 0_f64;
        let mut variable_rows = 0_usize;
        for operation in 0..OPERATIONS {
            let position = black_box(offset(operation, fixed.content_height()));
            let range = heights.visible_range(position, VIEWPORT, OVERSCAN);
            for (index, top, height) in heights.rows(range) {
                variable_checksum += black_box(index as f64 + f64::from(top) + f64::from(height));
                variable_rows += 1;
            }
        }
        let variable_scan_ns = started.elapsed().as_nanos();
        let started = Instant::now();
        let mut fixed_checksum = 0_f64;
        let mut fixed_rows = 0_usize;
        for operation in 0..OPERATIONS {
            let position = black_box(offset(operation, fixed.content_height()));
            for index in fixed.visible_range(position, VIEWPORT) {
                fixed_checksum += black_box(
                    index as f64 + f64::from(fixed.row_offset(index)) + f64::from(fixed.row_height),
                );
                fixed_rows += 1;
            }
        }
        let fixed_scan_ns = started.elapsed().as_nanos();
        assert_eq!(variable_rows, fixed_rows);
        assert_eq!(variable_checksum, fixed_checksum);
        assert!(variable_rows <= OPERATIONS * 20);

        let started = Instant::now();
        let mut expected_delta = 0_f64;
        for operation in 0..OPERATIONS {
            let index = operation * 7919 % count;
            let height = 16. + (operation % 97) as f32;
            expected_delta += f64::from(height - heights.row_height(index).unwrap());
            black_box(
                heights
                    .set_height(black_box(index), black_box(height))
                    .unwrap(),
            );
        }
        let update_ns = started.elapsed().as_nanos();
        let expected_total = (count as f64 * f64::from(ESTIMATE) + expected_delta) as f32;
        assert_eq!(heights.content_height(), expected_total);
        let started = Instant::now();
        for operation in 0..OPERATIONS {
            let index = operation * 7919 % count;
            let height = heights.row_height(index).unwrap();
            assert!(!black_box(
                heights
                    .set_height(black_box(index), black_box(height))
                    .unwrap()
            ));
        }
        let equal_update_ns = started.elapsed().as_nanos();

        let started = Instant::now();
        let mut heterogeneous_rows = 0_usize;
        let mut heterogeneous_checksum = 0_f64;
        for operation in 0..OPERATIONS {
            let position = black_box(offset(operation, heights.content_height()));
            for (index, top, height) in
                heights.rows(heights.visible_range(position, VIEWPORT, OVERSCAN))
            {
                heterogeneous_rows += 1;
                heterogeneous_checksum +=
                    black_box(index as f64 + f64::from(top) + f64::from(height));
            }
        }
        let heterogeneous_scan_ns = started.elapsed().as_nanos();
        assert!(heterogeneous_checksum.is_finite());
        let started = Instant::now();
        for added in 1..=OPERATIONS {
            assert!(black_box(heights.resize(black_box(count + added)).unwrap()));
        }
        let append_resize_ns = started.elapsed().as_nanos();
        assert_eq!(heights.count(), count + OPERATIONS);
        assert_eq!(
            heights.content_height(),
            (f64::from(expected_total) + OPERATIONS as f64 * f64::from(ESTIMATE)) as f32
        );
        assert_eq!(heights.row_height(count + OPERATIONS - 1), Some(ESTIMATE));
        // Logical vector element payload: f32 per row plus compensated f64 pair
        // per Fenwick slot. Excludes Vec headers, spare capacity and allocator/RSS.
        let logical_index_bytes = count * 4 + (count + 1) * 16;
        println!(
            "{{\"rows\":{count},\"operations\":{OPERATIONS},\"construction_ns\":{construction_ns},\"fixed_construction_ns\":{fixed_construction_ns},\"variable_scan_ns\":{variable_scan_ns},\"fixed_scan_ns\":{fixed_scan_ns},\"visited_equal_rows\":{variable_rows},\"update_ns\":{update_ns},\"equal_update_ns\":{equal_update_ns},\"heterogeneous_scan_ns\":{heterogeneous_scan_ns},\"visited_heterogeneous_rows\":{heterogeneous_rows},\"append_resize_ns\":{append_resize_ns},\"logical_index_payload_bytes_before_appends\":{logical_index_bytes},\"fixed_struct_bytes\":{},\"checksum\":{}}}",
            std::mem::size_of::<VirtualList>(),
            black_box(heterogeneous_checksum)
        );
    }
}
