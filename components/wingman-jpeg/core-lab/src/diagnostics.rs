//! Only compiled with the explicit `benchmark` feature. Not a stable API.
use crate::encoder::{DefaultOperations, Operations};
use crate::{quantization::QuantizationTable, DctBackend, QuantizationTableType};

#[inline]
pub fn dct(data: &mut [i16; 64], backend: DctBackend) {
    #[cfg(all(feature = "neon", target_arch = "aarch64"))]
    match backend {
        DctBackend::Neon | DctBackend::NeonQuant => return crate::neon_dct::fdct_neon(data),
        DctBackend::Auto | DctBackend::NeonFlat | DctBackend::NeonFlatQuant => {
            return crate::neon_dct::fdct_neon_flat(data)
        }
        _ => {}
    }
    let _ = backend;
    crate::fdct::fdct(data);
}

pub struct Quantizer(QuantizationTable);
impl Quantizer {
    pub fn new(quality: u8) -> Self {
        Self(QuantizationTable::new_with_quality(
            &QuantizationTableType::Default,
            quality,
            true,
        ))
    }
    #[inline]
    pub fn apply(&self, block: &[i16; 64], output: &mut [i16; 64], vector: bool) {
        #[cfg(all(feature = "neon", target_arch = "aarch64"))]
        if vector {
            return crate::neon_quant::quantize(block, output, &self.0);
        }
        let _ = vector;
        DefaultOperations::quantize_block(block, output, &self.0);
    }
}
