//! ARM64 implementation of the same LL&M integer factorization and rounding
//! used by upstream fdct.rs. Original IJG/mozjpeg attribution remains there;
//! this implementation changes the execution layout, not the DCT definition.
use core::arch::aarch64::*;

#[inline]
unsafe fn transpose(a: [int32x4_t; 4]) -> [int32x4_t; 4] {
    unsafe {
        let ab0 = vreinterpretq_s64_s32(vtrn1q_s32(a[0], a[1]));
        let ab1 = vreinterpretq_s64_s32(vtrn2q_s32(a[0], a[1]));
        let cd0 = vreinterpretq_s64_s32(vtrn1q_s32(a[2], a[3]));
        let cd1 = vreinterpretq_s64_s32(vtrn2q_s32(a[2], a[3]));
        [
            vreinterpretq_s32_s64(vcombine_s64(vget_low_s64(ab0), vget_low_s64(cd0))),
            vreinterpretq_s32_s64(vcombine_s64(vget_low_s64(ab1), vget_low_s64(cd1))),
            vreinterpretq_s32_s64(vcombine_s64(vget_high_s64(ab0), vget_high_s64(cd0))),
            vreinterpretq_s32_s64(vcombine_s64(vget_high_s64(ab1), vget_high_s64(cd1))),
        ]
    }
}

/// Four independent 1D transforms in the vector lanes. Keep every integer
/// operation and rounding point identical to the upstream two-pass transform.
#[inline]
unsafe fn transform<const FIRST: bool>(v: [int32x4_t; 8]) -> [int32x4_t; 8] {
    unsafe {
        let t0 = vaddq_s32(v[0], v[7]);
        let t7 = vsubq_s32(v[0], v[7]);
        let t1 = vaddq_s32(v[1], v[6]);
        let t6 = vsubq_s32(v[1], v[6]);
        let t2 = vaddq_s32(v[2], v[5]);
        let t5 = vsubq_s32(v[2], v[5]);
        let t3 = vaddq_s32(v[3], v[4]);
        let t4 = vsubq_s32(v[3], v[4]);
        let t10 = vaddq_s32(t0, t3);
        let t13 = vsubq_s32(t0, t3);
        let t11 = vaddq_s32(t1, t2);
        let t12 = vsubq_s32(t1, t2);
        let mut out = [vdupq_n_s32(0); 8];
        if FIRST {
            out[0] = vshlq_n_s32::<2>(vaddq_s32(t10, t11));
            out[4] = vshlq_n_s32::<2>(vsubq_s32(t10, t11));
        } else {
            out[0] = vrshrq_n_s32::<2>(vaddq_s32(t10, t11));
            out[4] = vrshrq_n_s32::<2>(vsubq_s32(t10, t11));
        }
        let z1 = vmulq_n_s32(vaddq_s32(t12, t13), 4433);
        let e2 = vmlaq_n_s32(z1, t13, 6270);
        let e6 = vmlaq_n_s32(z1, t12, -15137);
        let z1 = vaddq_s32(t4, t7);
        let z2 = vaddq_s32(t5, t6);
        let z3 = vaddq_s32(t4, t6);
        let z4 = vaddq_s32(t5, t7);
        let z5 = vmulq_n_s32(vaddq_s32(z3, z4), 9633);
        let a4 = vmulq_n_s32(t4, 2446);
        let a5 = vmulq_n_s32(t5, 16819);
        let a6 = vmulq_n_s32(t6, 25172);
        let a7 = vmulq_n_s32(t7, 12299);
        let z1 = vmulq_n_s32(z1, -7373);
        let z2 = vmulq_n_s32(z2, -20995);
        let z3 = vmlaq_n_s32(z5, z3, -16069);
        let z4 = vmlaq_n_s32(z5, z4, -3196);
        let o7 = vaddq_s32(vaddq_s32(a4, z1), z3);
        let o5 = vaddq_s32(vaddq_s32(a5, z2), z4);
        let o3 = vaddq_s32(vaddq_s32(a6, z2), z3);
        let o1 = vaddq_s32(vaddq_s32(a7, z1), z4);
        if FIRST {
            out[2] = vrshrq_n_s32::<11>(e2);
            out[6] = vrshrq_n_s32::<11>(e6);
            out[7] = vrshrq_n_s32::<11>(o7);
            out[5] = vrshrq_n_s32::<11>(o5);
            out[3] = vrshrq_n_s32::<11>(o3);
            out[1] = vrshrq_n_s32::<11>(o1);
        } else {
            out[2] = vrshrq_n_s32::<15>(e2);
            out[6] = vrshrq_n_s32::<15>(e6);
            out[7] = vrshrq_n_s32::<15>(o7);
            out[5] = vrshrq_n_s32::<15>(o5);
            out[3] = vrshrq_n_s32::<15>(o3);
            out[1] = vrshrq_n_s32::<15>(o1);
        }
        out
    }
}

/// Exact FDCT for level-shifted eight-bit JPEG samples (-128..=127).
pub fn fdct_neon(data: &mut [i16; 64]) {
    // Rust's AArch64 baseline includes NEON. All loads/stores below stay inside
    // the fixed-size input or intermediate arrays; unaligned access is valid.
    unsafe {
        let mut middle = [0_i32; 64];
        for row in [0, 4] {
            let left = transpose(core::array::from_fn(|i| {
                vmovl_s16(vld1_s16(data.as_ptr().add((row + i) * 8)))
            }));
            let right = transpose(core::array::from_fn(|i| {
                vmovl_s16(vld1_s16(data.as_ptr().add((row + i) * 8 + 4)))
            }));
            let transformed = transform::<true>([
                left[0], left[1], left[2], left[3], right[0], right[1], right[2], right[3],
            ]);
            let left = transpose([
                transformed[0],
                transformed[1],
                transformed[2],
                transformed[3],
            ]);
            let right = transpose([
                transformed[4],
                transformed[5],
                transformed[6],
                transformed[7],
            ]);
            for i in 0..4 {
                vst1q_s32(middle.as_mut_ptr().add((row + i) * 8), left[i]);
                vst1q_s32(middle.as_mut_ptr().add((row + i) * 8 + 4), right[i]);
            }
        }
        for column in [0, 4] {
            let transformed = transform::<false>(core::array::from_fn(|row| {
                vld1q_s32(middle.as_ptr().add(row * 8 + column))
            }));
            for (row, value) in transformed.into_iter().enumerate() {
                // Narrowing wraps exactly like the reference i32 -> i16 cast.
                vst1_s16(data.as_mut_ptr().add(row * 8 + column), vmovn_s32(value));
            }
        }
    }
}

/// A constant block c has DC=64*c in upstream's scaled domain, all AC=0.
/// The test examines the entire block, so an almost-flat block cannot take it.
pub fn fdct_neon_flat(data: &mut [i16; 64]) {
    if is_flat(data) {
        let dc = (i32::from(data[0]) * 64) as i16;
        data.fill(0);
        data[0] = dc;
    } else {
        fdct_neon(data);
    }
}

pub(crate) fn is_flat(data: &[i16; 64]) -> bool {
    // Exactly eight unaligned loads inside the fixed-size block.
    unsafe {
        let expected = vdupq_n_s16(data[0]);
        let mut same = vdupq_n_u16(u16::MAX);
        for offset in (0..64).step_by(8) {
            same = vandq_u16(
                same,
                vceqq_s16(vld1q_s16(data.as_ptr().add(offset)), expected),
            );
        }
        vminvq_u16(same) == u16::MAX
    }
}

/// Load a contiguous 8x8 byte tile and level-shift to signed samples. Padding
/// and subsampling decisions stay in the caller. Each row slice is checked
/// before its SIMD load; malformed ImageBuffer implementations cannot cause an
/// out-of-bounds read here. All 64 outputs are initialized before assume_init.
pub(crate) fn load_block(data: &[u8], start_x: usize, start_y: usize, width: usize) -> [i16; 64] {
    let mut output = core::mem::MaybeUninit::<[i16; 64]>::uninit();
    unsafe {
        let pointer = output.as_mut_ptr().cast::<i16>();
        for row in 0..8 {
            let offset = (start_y + row)
                .checked_mul(width)
                .and_then(|n| n.checked_add(start_x))
                .expect("JPEG block offset overflow");
            let source = &data[offset..offset.checked_add(8).expect("JPEG row overflow")];
            let values = vreinterpretq_s16_u16(vmovl_u8(vld1_u8(source.as_ptr())));
            vst1q_s16(pointer.add(row * 8), vsubq_s16(values, vdupq_n_s16(128)));
        }
        output.assume_init()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_odd_stride_and_unaligned_tiles_exactly() {
        let bytes: alloc::vec::Vec<u8> = (0..31 * 24).map(|i| (i * 73 % 256) as u8).collect();
        for x in 0..24 {
            for y in 0..17 {
                let actual = load_block(&bytes, x, y, 31);
                for row in 0..8 {
                    for column in 0..8 {
                        assert_eq!(
                            actual[row * 8 + column],
                            i16::from(bytes[(y + row) * 31 + x + column]) - 128
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[should_panic]
    fn rejects_truncated_tile_before_reading_out_of_bounds() {
        let _ = load_block(&[0; 63], 0, 0, 8);
    }

    fn check(input: [i16; 64]) {
        let mut reference = input;
        crate::fdct::fdct(&mut reference);
        let mut neon = input;
        fdct_neon(&mut neon);
        assert_eq!(neon, reference);
        let mut flat = input;
        fdct_neon_flat(&mut flat);
        assert_eq!(flat, reference);
    }

    #[test]
    fn constants_impulses_and_every_single_pixel_perturbation_match() {
        for c in -128..=127 {
            check([c; 64]);
            for i in 0..64 {
                let mut input = [c; 64];
                input[i] = if c == 127 { -128 } else { 127 };
                check(input);
            }
        }
    }

    #[test]
    fn million_deterministic_random_and_structured_blocks_match() {
        let mut seed = 0x6789abcd_u32;
        for n in 0..1_000_000 {
            let mut input = [0; 64];
            for (i, value) in input.iter_mut().enumerate() {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                *value = match n % 8 {
                    0 => {
                        if i % 2 == 0 {
                            -128
                        } else {
                            127
                        }
                    }
                    1 => (i * 4) as i16 - 128,
                    2 => (seed % 5) as i16 - 2,
                    _ => (seed as u8) as i16 - 128,
                };
            }
            check(input);
        }
    }
}
