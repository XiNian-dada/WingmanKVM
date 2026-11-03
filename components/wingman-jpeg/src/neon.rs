use core::arch::aarch64::*;

/// Same 16-bit fixed-point coefficients and 0x7fff rounding as upstream.
/// Accumulators stay in u32 range, including both chroma subtraction stages.
#[target_feature(enable = "neon")]
unsafe fn eight(rgb: *const u8, y: *mut u8, cb: *mut u8, cr: *mut u8) {
    unsafe {
        let channels = vld3_u8(rgb);
        let r = vmovl_u8(channels.0);
        let g = vmovl_u8(channels.1);
        let b = vmovl_u8(channels.2);
        let mut ys = [vdup_n_u16(0); 2];
        let mut cbs = ys;
        let mut crs = ys;
        for (i, (r, g, b)) in [
            (vget_low_u16(r), vget_low_u16(g), vget_low_u16(b)),
            (vget_high_u16(r), vget_high_u16(g), vget_high_u16(b)),
        ]
        .into_iter()
        .enumerate()
        {
            let yy = vmlal_n_u16(vmlal_n_u16(vmull_n_u16(r, 19595), g, 38470), b, 7471);
            let yy = vaddq_u32(yy, vdupq_n_u32(0x7fff));
            let cc = vmlal_n_u16(vdupq_n_u32((128 << 16) + 0x7fff), b, 32768);
            let cc = vmlsl_n_u16(vmlsl_n_u16(cc, r, 11059), g, 21709);
            let rr = vmlal_n_u16(vdupq_n_u32((128 << 16) + 0x7fff), r, 32768);
            let rr = vmlsl_n_u16(vmlsl_n_u16(rr, g, 27439), b, 5329);
            ys[i] = vshrn_n_u32::<16>(yy);
            cbs[i] = vshrn_n_u32::<16>(cc);
            crs[i] = vshrn_n_u32::<16>(rr);
        }
        vst1_u8(y, vmovn_u16(vcombine_u16(ys[0], ys[1])));
        vst1_u8(cb, vmovn_u16(vcombine_u16(cbs[0], cbs[1])));
        vst1_u8(cr, vmovn_u16(vcombine_u16(crs[0], crs[1])));
    }
}

pub(crate) fn convert(rgb: &[u8], y: &mut [u8], cb: &mut [u8], cr: &mut [u8]) {
    assert_eq!(rgb.len(), y.len() * 3);
    assert_eq!(cb.len(), y.len());
    assert_eq!(cr.len(), y.len());
    let count = y.len() / 8 * 8;
    for offset in (0..count).step_by(8) {
        // The loop bounds ensure 24 input bytes and 8 bytes in each disjoint
        // output slice are valid. NEON loads/stores permit unaligned pointers.
        unsafe {
            eight(
                rgb.as_ptr().add(offset * 3),
                y.as_mut_ptr().add(offset),
                cb.as_mut_ptr().add(offset),
                cr.as_mut_ptr().add(offset),
            );
        }
    }
    crate::color::scalar(
        &rgb[count * 3..],
        &mut y[count..],
        &mut cb[count..],
        &mut cr[count..],
    );
}
