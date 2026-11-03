//! Exact reciprocal quantization in natural coefficient order, then zigzag.
use crate::{quantization::QuantizationTable, writer::ZIGZAG};
use core::arch::aarch64::*;

pub(crate) fn quantize(block: &[i16; 64], output: &mut [i16; 64], table: &QuantizationTable) {
    let mut natural = [0_i16; 64];
    // Reciprocal quantization is Q15. SQDMULH performs the shift and multiply
    // on eight i16 lanes directly. For sums >=32768, reinterpretation subtracts
    // 65536; adding 2*reciprocal restores the exact unsigned product / 32768.
    // Accepted tables have reciprocal<=4096, correction<=1020, so no lane sum
    // exceeds 33788 and SQDMULH cannot saturate (reciprocal is positive).
    unsafe {
        for offset in (0..64).step_by(8) {
            let value = vld1q_s16(block.as_ptr().add(offset));
            let reciprocal = vld1q_s16(table.reciprocals16.as_ptr().add(offset));
            let correction = vld1q_u16(table.corrections16.as_ptr().add(offset));
            let magnitude = vreinterpretq_u16_s16(vabsq_s16(value));
            let sum = vaddq_u16(magnitude, correction);
            let signed_product = vqdmulhq_s16(vreinterpretq_s16_u16(sum), reciprocal);
            let high = vcgtq_u16(sum, vdupq_n_u16(32767));
            let extra = vandq_s16(vreinterpretq_s16_u16(high), vshlq_n_s16::<1>(reciprocal));
            let magnitude = vaddq_s16(signed_product, extra);
            let result = vbslq_s16(
                vcltq_s16(value, vdupq_n_s16(0)),
                vnegq_s16(magnitude),
                magnitude,
            );
            vst1q_s16(natural.as_mut_ptr().add(offset), result);
        }
    }
    for (i, &position) in ZIGZAG.iter().enumerate() {
        output[i] = natural[usize::from(position)];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::QuantizationTableType;

    #[test]
    fn every_i16_coefficient_matches_reciprocal_reference() {
        for quality in [1, 5, 20, 50, 81, 95, 100] {
            for luma in [false, true] {
                let table = QuantizationTable::new_with_quality(
                    &QuantizationTableType::Default,
                    quality,
                    luma,
                );
                for value in i16::MIN..=i16::MAX {
                    let block = [value; 64];
                    let mut output = [0; 64];
                    quantize(&block, &mut output, &table);
                    for (i, &position) in ZIGZAG.iter().enumerate() {
                        assert_eq!(output[i], table.quantize(value, usize::from(position)));
                    }
                }
            }
        }
    }
}
