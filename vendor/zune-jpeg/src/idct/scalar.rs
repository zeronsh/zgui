/*
 * Copyright (c) 2023.
 *
 * This software is free software;
 *
 * You can redistribute it or modify it under terms of the MIT, Apache License or Zlib license
 */

//! Platform independent IDCT algorithm
//!
//! Not as fast as AVX one.

const SCALE_BITS: i32 = 512 + 65536 + (128 << 17);

#[inline(always)]
fn wa(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}

#[inline(always)]
fn ws(a: i32, b: i32) -> i32 {
    a.wrapping_sub(b)
}

#[inline(always)]
fn wm(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b)
}

#[inline]
pub fn idct_int_1x1(in_vector: &mut [i32; 64], mut out_vector: &mut [i16], stride: usize) {
    let coeff = ((wa(wa(in_vector[0], 4), 1024) >> 3).clamp(0, 255)) as i16;

    out_vector[..8].fill(coeff);
    for _ in 0..7 {
        out_vector = &mut out_vector[stride..];
        out_vector[..8].fill(coeff);

    }
}

#[allow(unused_assignments)]
#[allow(
    clippy::too_many_lines,
    clippy::op_ref,
    clippy::cast_possible_truncation
)]
pub fn idct_int(in_vector: &mut [i32; 64], out_vector: &mut [i16], stride: usize) {
    let mut pos = 0;
    let mut i = 0;

    if &in_vector[1..] == &[0_i32; 63] {
        return idct_int_1x1(in_vector, out_vector, stride);
    }

    // vertical pass
    for ptr in 0..8 {
        let p2 = in_vector[ptr + 16];
        let p3 = in_vector[ptr + 48];

        let p1 = wm(wa(p2, p3), 2217);

        let t2 = wa(p1, wm(p3, -7567));
        let t3 = wa(p1, wm(p2, 3135));

        let p2 = in_vector[ptr];
        let p3 = in_vector[32 + ptr];

        let t0 = fsh(wa(p2, p3));
        let t1 = fsh(ws(p2, p3));

        let x0 = wa(wa(t0, t3), 512);
        let x3 = wa(ws(t0, t3), 512);
        let x1 = wa(wa(t1, t2), 512);
        let x2 = wa(ws(t1, t2), 512);

        let mut t0 = in_vector[ptr + 56];
        let mut t1 = in_vector[ptr + 40];
        let mut t2 = in_vector[ptr + 24];
        let mut t3 = in_vector[ptr + 8];

        let p3 = wa(t0, t2);
        let p4 = wa(t1, t3);
        let p1 = wa(t0, t3);
        let p2 = wa(t1, t2);
        let p5 = wm(wa(p3, p4), 4816);

        t0 = wm(t0, 1223);
        t1 = wm(t1, 8410);
        t2 = wm(t2, 12586);
        t3 = wm(t3, 6149);

        let p1 = wa(p5, wm(p1, -3685));
        let p2 = wa(p5, wm(p2, -10497));
        let p3 = wm(p3, -8034);
        let p4 = wm(p4, -1597);

        t3 = wa(t3, wa(p1, p4));
        t2 = wa(t2, wa(p2, p3));
        t1 = wa(t1, wa(p2, p4));
        t0 = wa(t0, wa(p1, p3));

        in_vector[ptr] = ws(wa(x0, t3), 0) >> 10;
        in_vector[ptr + 8] = ws(wa(x1, t2), 0) >> 10;
        in_vector[ptr + 16] = ws(wa(x2, t1), 0) >> 10;
        in_vector[ptr + 24] = ws(wa(x3, t0), 0) >> 10;
        in_vector[ptr + 32] = ws(ws(x3, t0), 0) >> 10;
        in_vector[ptr + 40] = ws(ws(x2, t1), 0) >> 10;
        in_vector[ptr + 48] = ws(ws(x1, t2), 0) >> 10;
        in_vector[ptr + 56] = ws(ws(x0, t3), 0) >> 10;
    }

    // horizontal pass
    while i < 64 {
        let p2 = in_vector[i + 2];
        let p3 = in_vector[i + 6];

        let p1 = wm(wa(p2, p3), 2217);
        let t2 = wa(p1, wm(p3, -7567));
        let t3 = wa(p1, wm(p2, 3135));

        let p2 = in_vector[i];
        let p3 = in_vector[i + 4];

        let t0 = fsh(wa(p2, p3));
        let t1 = fsh(ws(p2, p3));

        let x0 = wa(wa(t0, t3), SCALE_BITS);
        let x3 = wa(ws(t0, t3), SCALE_BITS);
        let x1 = wa(wa(t1, t2), SCALE_BITS);
        let x2 = wa(ws(t1, t2), SCALE_BITS);

        let mut t0 = in_vector[i + 7];
        let mut t1 = in_vector[i + 5];
        let mut t2 = in_vector[i + 3];
        let mut t3 = in_vector[i + 1];

        let p3 = wa(t0, t2);
        let p4 = wa(t1, t3);
        let p1 = wa(t0, t3);
        let p2 = wa(t1, t2);
        let p5 = wm(wa(p3, p4), f2f(1.175875602));

        t0 = wm(t0, 1223);
        t1 = wm(t1, 8410);
        t2 = wm(t2, 12586);
        t3 = wm(t3, 6149);

        let p1 = wa(p5, wm(p1, -3685));
        let p2 = wa(p5, wm(p2, -10497));
        let p3 = wm(p3, -8034);
        let p4 = wm(p4, -1597);

        t3 = wa(t3, wa(p1, p4));
        t2 = wa(t2, wa(p2, p3));
        t1 = wa(t1, wa(p2, p4));
        t0 = wa(t0, wa(p1, p3));

        // to prevent some bad images from crashing
        let mut tmp = [0; 8];

        let out: &mut [i16; 8] = out_vector
            .get_mut(pos..pos + 8)
            .unwrap_or(&mut tmp)
            .try_into()
            .unwrap();

        out[0] = clamp(wa(x0, t3) >> 17);
        out[1] = clamp(wa(x1, t2) >> 17);
        out[2] = clamp(wa(x2, t1) >> 17);
        out[3] = clamp(wa(x3, t0) >> 17);
        out[4] = clamp(ws(x3, t0) >> 17);
        out[5] = clamp(ws(x2, t1) >> 17);
        out[6] = clamp(ws(x1, t2) >> 17);
        out[7] = clamp(ws(x0, t3) >> 17);

        i += 8;
        pos += stride;
    }
}

#[inline]
#[allow(clippy::cast_possible_truncation)]
/// Multiply a number by 4096
fn f2f(x: f32) -> i32 {
    (x * 4096.0 + 0.5) as i32
}

#[inline]
/// Multiply a number by 4096
fn fsh(x: i32) -> i32 {
    x << 12
}

/// Clamp values between 0 and 255
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn clamp(a: i32) -> i16 {
    a.clamp(0, 255) as i16
}

/// IDCT assuming only the upper 4x4 is filled.
pub fn idct4x4(in_vector: &mut [i32; 64], out_vector: &mut [i16], stride: usize) {
    let mut pos = 0;

    // vertical pass
    for ptr in 0..4 {
        let i0 = wa(fsh(in_vector[ptr]), 512);
        let i2 = in_vector[ptr + 16];

        let p1 = wm(i2, 2217);
        let p3 = wm(i2, 5352);

        let x0 = wa(i0, p3);
        let x1 = wa(i0, p1);
        let x2 = ws(i0, p1);
        let x3 = ws(i0, p3);

        // odd part
        let i4 = in_vector[ptr + 24];
        let i3 = in_vector[ptr + 8];

        let p5 = wm(wa(i4, i3), 4816);

        let p1 = wa(p5, wm(i3, -3685));
        let p2 = wa(p5, wm(i4, -10497));

        let t3 = wa(p5, wm(i3, 867));
        let t2 = wa(p5, wm(i4, -5945));

        let t1 = wa(p2, wm(i3, -1597));
        let t0 = wa(p1, wm(i4, -8034));

        in_vector[ptr] = wa(x0, t3) >> 10;
        in_vector[ptr + 8] = wa(x1, t2) >> 10;
        in_vector[ptr + 16] = wa(x2, t1) >> 10;
        in_vector[ptr + 24] = wa(x3, t0) >> 10;
        in_vector[ptr + 32] = ws(x3, t0) >> 10;
        in_vector[ptr + 40] = ws(x2, t1) >> 10;
        in_vector[ptr + 48] = ws(x1, t2) >> 10;
        in_vector[ptr + 56] = ws(x0, t3) >> 10;
    }

    // horizontal pass
    for i in (0..8).map(|i| 8 * i) {
        let i2 = in_vector[i + 2];
        let i0 = in_vector[i];

        let t0 = wa(fsh(i0), SCALE_BITS);
        let t2 = wm(i2, 2217);
        let t3 = wm(i2, 5352);

        let x0 = wa(t0, t3);
        let x3 = ws(t0, t3);
        let x1 = wa(t0, t2);
        let x2 = ws(t0, t2);

        // odd part
        let i3 = in_vector[i + 3];
        let i1 = in_vector[i + 1];

        let p5 = wm(wa(i3, i1), f2f(1.175875602));

        let p1 = wa(p5, wm(i1, -3685));
        let p2 = wa(p5, wm(i3, -10497));

        let t3 = wa(p5, wm(i1, 867));
        let t2 = wa(p5, wm(i3, -5945));

        let t1 = wa(p2, wm(i1, -1597));
        let t0 = wa(p1, wm(i3, -8034));

        // to prevent some bad images from crashing
        let mut tmp = [0; 8];

        let out: &mut [i16; 8] = out_vector
            .get_mut(pos..pos + 8)
            .unwrap_or(&mut tmp)
            .try_into()
            .unwrap();

        out.copy_from_slice(&[
            clamp(wa(x0, t3) >> 17),
            clamp(wa(x1, t2) >> 17),
            clamp(wa(x2, t1) >> 17),
            clamp(wa(x3, t0) >> 17),
            clamp(ws(x3, t0) >> 17),
            clamp(ws(x2, t1) >> 17),
            clamp(ws(x1, t2) >> 17),
            clamp(ws(x0, t3) >> 17)
        ]);

        pos += stride;
    }

    in_vector[32..36].fill(0);
    in_vector[40..44].fill(0);
    in_vector[48..52].fill(0);
    in_vector[56..60].fill(0);
}

/// zgx: the IDCT of a scaled decode (see `JpegDecoder::set_scale`): from a
/// dequantized 8x8 block, write a `block_w` by `block_h` block of pixels (each
/// 8, 4, 2 or 1) at `stride`, separably, with libjpeg's kernels: `jidctred.c`'s
/// for 4, 2 and 1 points and `jidctint.c`'s for 8. A reduced kernel reads all
/// the coefficients but those whose cosines cancel over a group of pixels, so
/// each output is the mean of the 8 / n full-size pixels it replaces, to
/// rounding (what shrinking the full decode by area would give, without making
/// it). `in_vector` is only read.
pub fn idct_scaled(
    block_w: usize, block_h: usize, in_vector: &mut [i32; 64], out_vector: &mut [i16],
    stride: usize
) {
    // One body per block shape, so the kernels and shifts are constants.
    match block_w {
        8 => idct_scaled_w::<8>(block_h, in_vector, out_vector, stride),
        4 => idct_scaled_w::<4>(block_h, in_vector, out_vector, stride),
        2 => idct_scaled_w::<2>(block_h, in_vector, out_vector, stride),
        _ => idct_scaled_w::<1>(block_h, in_vector, out_vector, stride)
    }
}

#[inline(always)]
fn idct_scaled_w<const W: usize>(
    block_h: usize, in_vector: &[i32; 64], out_vector: &mut [i16], stride: usize
) {
    match block_h {
        8 => idct_scaled_wh::<W, 8>(in_vector, out_vector, stride),
        4 => idct_scaled_wh::<W, 4>(in_vector, out_vector, stride),
        2 => idct_scaled_wh::<W, 2>(in_vector, out_vector, stride),
        _ => idct_scaled_wh::<W, 1>(in_vector, out_vector, stride)
    }
}

#[allow(clippy::cast_possible_truncation)]
fn idct_scaled_wh<const W: usize, const H: usize>(
    in_vector: &[i32; 64], out_vector: &mut [i16], stride: usize
) {
    const CONST_BITS: u32 = 13;
    const PASS1_BITS: u32 = 2;
    // A kernel of n points has 8 / n times the 8-point kernel's gain.
    let shift1 = CONST_BITS - PASS1_BITS + (8 / H).trailing_zeros();
    let shift2 = CONST_BITS + PASS1_BITS + 3 + (8 / W).trailing_zeros();

    // Pass 1: columns into an H by 8 work array, at `PASS1_BITS`. A column
    // the row kernel won't read is left at zero.
    let mut workspace = [[0_i64; 8]; H];
    for col in 0..8 {
        if !reduced_kernel_reads(W, col) {
            continue;
        }
        let column: [i64; 8] = core::array::from_fn(|r| i64::from(in_vector[r * 8 + col]));
        // As libjpeg: a column of only its DC is that DC all the way down.
        if (1..8).all(|r| column[r] == 0 || !reduced_kernel_reads(H, r)) {
            for row in &mut workspace {
                row[col] = column[0] << PASS1_BITS;
            }
            continue;
        }
        let mut out = [0_i64; 8];
        reduced_kernel(H, &column, &mut out);
        for (row, v) in workspace.iter_mut().zip(out) {
            row[col] = descale(v, shift1);
        }
    }

    // Pass 2: rows, with the 2-D IDCT's 1/8, the level shift and a clamp.
    for (r, row) in workspace.iter().enumerate() {
        let mut out = [0_i64; 8];
        reduced_kernel(W, row, &mut out);
        let mut pixels = [0_i16; W];
        for (p, v) in pixels.iter_mut().zip(out) {
            *p = (descale(v, shift2).saturating_add(128)).clamp(0, 255) as i16;
        }
        // Drop what falls outside, as the full IDCT does for bad images.
        if let Some(dest) = out_vector.get_mut(r * stride..r * stride + W) {
            dest.copy_from_slice(&pixels);
        }
    }
}

/// zgx: `idct_scaled` for the block at (`bx`, `by`), in blocks, of a component
/// `full_w` by `full_h` samples. A block straddling the image's right or bottom
/// edge holds the encoder's padding, which a reduced IDCT would average in:
/// there the block is made whole and only the samples inside are averaged, as
/// shrinking the full decode would. Blocks inside take `idct_scaled`.
#[allow(clippy::too_many_arguments, clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
pub fn idct_scaled_at(
    block_w: usize, block_h: usize, bx: usize, by: usize, full_w: usize, full_h: usize,
    in_vector: &mut [i32; 64], out_vector: &mut [i16], stride: usize
) {
    let real_w = full_w.saturating_sub(bx * 8);
    let real_h = full_h.saturating_sub(by * 8);
    // Inside, or wholly outside the image (MCU padding, never shown).
    if (real_w >= 8 || real_w == 0) && (real_h >= 8 || real_h == 0) {
        return idct_scaled(block_w, block_h, in_vector, out_vector, stride);
    }
    let (real_w, real_h) = (real_w.clamp(1, 8), real_h.clamp(1, 8));
    let mut coeffs = *in_vector;
    let mut full = [0_i16; 64];
    idct_int(&mut coeffs, &mut full, 8);

    let (gw, gh) = (8 / block_w, 8 / block_h);
    for oy in 0..block_h {
        // A group wholly outside is never shown; average it whole.
        let y0 = oy * gh;
        let y1 = if y0 < real_h { ((oy + 1) * gh).min(real_h) } else { (oy + 1) * gh };
        let mut pixels = [0_i16; 8];
        for (ox, p) in pixels.iter_mut().enumerate().take(block_w) {
            let x0 = ox * gw;
            let x1 = if x0 < real_w { ((ox + 1) * gw).min(real_w) } else { (ox + 1) * gw };
            let mut sum = 0_i32;
            for y in y0..y1 {
                for x in x0..x1 {
                    sum += i32::from(full[y * 8 + x]);
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as i32;
            *p = ((sum + n / 2) / n) as i16;
        }
        if let Some(dest) = out_vector.get_mut(oy * stride..oy * stride + block_w) {
            dest.copy_from_slice(&pixels[..block_w]);
        }
    }
}

/// zgx: libjpeg's `DESCALE`: shift right, rounding.
#[inline(always)]
fn descale(x: i64, n: u32) -> i64 {
    (x + (1 << (n - 1))) >> n
}

/// zgx: whether the `n`-point kernel reads coefficient `k`: the 4-point one
/// skips 4, the 2-point one 2, 4 and 6 (they sum to zero over its pixels).
#[inline(always)]
fn reduced_kernel_reads(n: usize, k: usize) -> bool {
    match n {
        8 => true,
        4 => k != 4,
        2 => k % 2 == 1 || k == 0,
        _ => k == 0
    }
}

/// zgx: one dimension of a scaled IDCT: `n` (8, 4, 2 or 1) outputs from the 8
/// coefficients of `x`, at libjpeg's `CONST_BITS` (13) and with `8 / n` times
/// the gain of the 8-point kernel (each output is the sum of the 8 / n
/// outputs it stands for).
#[inline(always)]
fn reduced_kernel(n: usize, x: &[i64; 8], out: &mut [i64; 8]) {
    const CONST_BITS: u32 = 13;
    const FIX_0_211164243: i64 = 1730;
    const FIX_0_298631336: i64 = 2446;
    const FIX_0_390180644: i64 = 3196;
    const FIX_0_509795579: i64 = 4176;
    const FIX_0_541196100: i64 = 4433;
    const FIX_0_601344887: i64 = 4926;
    const FIX_0_720959822: i64 = 5906;
    const FIX_0_765366865: i64 = 6270;
    const FIX_0_850430095: i64 = 6967;
    const FIX_0_899976223: i64 = 7373;
    const FIX_1_061594337: i64 = 8697;
    const FIX_1_175875602: i64 = 9633;
    const FIX_1_272758580: i64 = 10426;
    const FIX_1_451774981: i64 = 11893;
    const FIX_1_501321110: i64 = 12299;
    const FIX_1_847759065: i64 = 15137;
    const FIX_1_961570560: i64 = 16069;
    const FIX_2_053119869: i64 = 16819;
    const FIX_2_172734803: i64 = 17799;
    const FIX_2_562915447: i64 = 20995;
    const FIX_3_072711026: i64 = 25172;
    const FIX_3_624509785: i64 = 29692;

    match n {
        8 => {
            // jidctint.c's jpeg_idct_islow, one pass.
            let z2 = x[2];
            let z3 = x[6];
            let z1 = (z2 + z3) * FIX_0_541196100;
            let tmp2 = z1 - z3 * FIX_1_847759065;
            let tmp3 = z1 + z2 * FIX_0_765366865;
            let tmp0 = (x[0] + x[4]) << CONST_BITS;
            let tmp1 = (x[0] - x[4]) << CONST_BITS;
            let tmp10 = tmp0 + tmp3;
            let tmp13 = tmp0 - tmp3;
            let tmp11 = tmp1 + tmp2;
            let tmp12 = tmp1 - tmp2;

            let (t0, t1, t2, t3) = (x[7], x[5], x[3], x[1]);
            let z1 = t0 + t3;
            let z2 = t1 + t2;
            let z3 = t0 + t2;
            let z4 = t1 + t3;
            let z5 = (z3 + z4) * FIX_1_175875602;
            let z1 = -z1 * FIX_0_899976223;
            let z2 = -z2 * FIX_2_562915447;
            let z3 = -z3 * FIX_1_961570560 + z5;
            let z4 = -z4 * FIX_0_390180644 + z5;
            let t0 = t0 * FIX_0_298631336 + z1 + z3;
            let t1 = t1 * FIX_2_053119869 + z2 + z4;
            let t2 = t2 * FIX_3_072711026 + z2 + z3;
            let t3 = t3 * FIX_1_501321110 + z1 + z4;

            *out = [
                tmp10 + t3,
                tmp11 + t2,
                tmp12 + t1,
                tmp13 + t0,
                tmp13 - t0,
                tmp12 - t1,
                tmp11 - t2,
                tmp10 - t3
            ];
        }
        4 => {
            // jidctred.c's jpeg_idct_4x4, one pass.
            let tmp0 = x[0] << (CONST_BITS + 1);
            let tmp2 = x[2] * FIX_1_847759065 - x[6] * FIX_0_765366865;
            let tmp10 = tmp0 + tmp2;
            let tmp12 = tmp0 - tmp2;

            let (z1, z2, z3, z4) = (x[7], x[5], x[3], x[1]);
            let tmp0 = -z1 * FIX_0_211164243 + z2 * FIX_1_451774981 - z3 * FIX_2_172734803
                + z4 * FIX_1_061594337;
            let tmp2 = -z1 * FIX_0_509795579 - z2 * FIX_0_601344887 + z3 * FIX_0_899976223
                + z4 * FIX_2_562915447;

            out[..4].copy_from_slice(&[tmp10 + tmp2, tmp12 + tmp0, tmp12 - tmp0, tmp10 - tmp2]);
        }
        2 => {
            // jidctred.c's jpeg_idct_2x2, one pass.
            let tmp10 = x[0] << (CONST_BITS + 2);
            let tmp0 = -x[7] * FIX_0_720959822 + x[5] * FIX_0_850430095 - x[3] * FIX_1_272758580
                + x[1] * FIX_3_624509785;

            out[..2].copy_from_slice(&[tmp10 + tmp0, tmp10 - tmp0]);
        }
        _ => {
            // jidctred.c's jpeg_idct_1x1: the DC.
            out[0] = x[0] << (CONST_BITS + 3);
        }
    }
}
