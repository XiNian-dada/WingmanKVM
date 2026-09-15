use crate::config::VirtualMonitorMode;

const EDID_BLOCK_LEN: usize = 128;
const EDID_RAM_LEN: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdidImage([u8; EDID_RAM_LEN]);

impl EdidImage {
    pub fn for_mode(mode: VirtualMonitorMode) -> Option<Self> {
        let timing = match mode {
            VirtualMonitorMode::Unmanaged => return None,
            VirtualMonitorMode::Hd1080p60 => EdidTiming {
                width: 1920,
                height: 1080,
                pixel_clock_10khz: 14_850,
                horizontal_blank: 280,
                vertical_blank: 45,
                horizontal_sync_offset: 88,
                horizontal_sync_width: 44,
                vertical_sync_offset: 4,
                vertical_sync_width: 5,
                cea_vic: 16,
                product_code: 0x1080,
            },
            VirtualMonitorMode::Hd720p60 => EdidTiming {
                width: 1280,
                height: 720,
                pixel_clock_10khz: 7_425,
                horizontal_blank: 370,
                vertical_blank: 30,
                horizontal_sync_offset: 110,
                horizontal_sync_width: 40,
                vertical_sync_offset: 5,
                vertical_sync_width: 5,
                cea_vic: 4,
                product_code: 0x0720,
            },
        };
        Some(Self(build_edid(timing)))
    }

    pub fn as_bytes(&self) -> &[u8; EDID_RAM_LEN] {
        &self.0
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.0[..8] != [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00] {
            return Err("invalid EDID header");
        }
        if self.0[126] != 1 {
            return Err("EDID must contain exactly one CTA extension");
        }
        for block in self.0.chunks_exact(EDID_BLOCK_LEN) {
            if block.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)) != 0 {
                return Err("invalid EDID checksum");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct EdidTiming {
    width: u16,
    height: u16,
    pixel_clock_10khz: u16,
    horizontal_blank: u16,
    vertical_blank: u16,
    horizontal_sync_offset: u16,
    horizontal_sync_width: u16,
    vertical_sync_offset: u8,
    vertical_sync_width: u8,
    cea_vic: u8,
    product_code: u16,
}

fn build_edid(timing: EdidTiming) -> [u8; EDID_RAM_LEN] {
    let mut edid = [0_u8; EDID_RAM_LEN];
    let base = &mut edid[..EDID_BLOCK_LEN];
    base[..8].copy_from_slice(&[0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00]);
    base[8..10].copy_from_slice(&manufacturer_id(*b"WKM").to_be_bytes());
    base[10..12].copy_from_slice(&timing.product_code.to_le_bytes());
    base[12..16].copy_from_slice(&(u32::from(timing.product_code) | 0x574b_0000).to_le_bytes());
    base[16] = 1;
    base[17] = 36; // 1990 + 36 = 2026
    base[18] = 1;
    base[19] = 4;
    base[20] = 0x80; // digital input
    base[21] = 51;
    base[22] = 29;
    base[23] = 0x78; // gamma 2.2
    base[24] = 0x06; // sRGB + preferred timing
    base[25..35].copy_from_slice(&[0xee, 0x91, 0xa3, 0x54, 0x4c, 0x99, 0x26, 0x0f, 0x50, 0x54]);
    for standard_timing in base[38..54].chunks_exact_mut(2) {
        standard_timing.copy_from_slice(&[0x01, 0x01]);
    }
    write_detailed_timing(&mut base[54..72], timing);
    write_text_descriptor(&mut base[72..90], 0xfc, b"WingmanKVM");
    let serial = if timing.width == 1920 {
        b"WMK-1080P60".as_slice()
    } else {
        b"WMK-720P60".as_slice()
    };
    write_text_descriptor(&mut base[90..108], 0xff, serial);
    // An unused descriptor avoids advertising range-derived or fallback modes.
    base[108..126].copy_from_slice(&[
        0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00,
    ]);
    base[126] = 1;
    set_checksum(base);

    let extension = &mut edid[EDID_BLOCK_LEN..];
    extension[0] = 0x02; // CTA-861 extension
    extension[1] = 0x03;
    extension[2] = 14; // start of detailed timings (none are present)
    extension[3] = 0x40; // basic audio
    extension[4] = 0x41; // video data block, one VIC
    extension[5] = timing.cea_vic | 0x80; // the sole VIC is native
    extension[6..10].copy_from_slice(&[0x23, 0x09, 0x07, 0x07]); // 2ch LPCM
    extension[10..14].copy_from_slice(&[0x83, 0x01, 0x00, 0x00]); // front L/R
    set_checksum(extension);
    edid
}

fn manufacturer_id(name: [u8; 3]) -> u16 {
    let letter = |byte: u8| u16::from(byte.saturating_sub(b'A').saturating_add(1) & 0x1f);
    (letter(name[0]) << 10) | (letter(name[1]) << 5) | letter(name[2])
}

fn write_detailed_timing(bytes: &mut [u8], timing: EdidTiming) {
    bytes[..2].copy_from_slice(&timing.pixel_clock_10khz.to_le_bytes());
    bytes[2] = timing.width as u8;
    bytes[3] = timing.horizontal_blank as u8;
    bytes[4] = (((timing.width >> 8) as u8) << 4) | ((timing.horizontal_blank >> 8) as u8);
    bytes[5] = timing.height as u8;
    bytes[6] = timing.vertical_blank as u8;
    bytes[7] = (((timing.height >> 8) as u8) << 4) | ((timing.vertical_blank >> 8) as u8);
    bytes[8] = timing.horizontal_sync_offset as u8;
    bytes[9] = timing.horizontal_sync_width as u8;
    bytes[10] = (timing.vertical_sync_offset << 4) | timing.vertical_sync_width;
    bytes[11] = (((timing.horizontal_sync_offset >> 8) as u8) << 6)
        | (((timing.horizontal_sync_width >> 8) as u8) << 4)
        | ((timing.vertical_sync_offset >> 4) << 2)
        | (timing.vertical_sync_width >> 4);
    bytes[12] = 0xfd; // 509 mm x 286 mm
    bytes[13] = 0x1e;
    bytes[14] = 0x21;
    bytes[17] = 0x1e; // digital separate sync, positive H/V
}

fn write_text_descriptor(bytes: &mut [u8], tag: u8, text: &[u8]) {
    bytes[..5].copy_from_slice(&[0x00, 0x00, 0x00, tag, 0x00]);
    bytes[5..].fill(b' ');
    let length = text.len().min(12);
    bytes[5..5 + length].copy_from_slice(&text[..length]);
    bytes[5 + length] = b'\n';
}

fn set_checksum(block: &mut [u8]) {
    let checksum_index = block.len() - 1;
    block[checksum_index] = 0;
    let sum = block.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
    block[checksum_index] = 0_u8.wrapping_sub(sum);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded_preferred_size(edid: &EdidImage) -> (u16, u16) {
        let dtd = &edid.as_bytes()[54..72];
        let width = u16::from(dtd[2]) | (u16::from(dtd[4] >> 4) << 8);
        let height = u16::from(dtd[5]) | (u16::from(dtd[7] >> 4) << 8);
        (width, height)
    }

    #[test]
    fn strict_profiles_are_valid_and_advertise_only_the_selected_vic() {
        for (mode, size, vic) in [
            (VirtualMonitorMode::Hd1080p60, (1920, 1080), 16),
            (VirtualMonitorMode::Hd720p60, (1280, 720), 4),
        ] {
            let edid = EdidImage::for_mode(mode).unwrap();
            assert_eq!(edid.validate(), Ok(()));
            assert_eq!(decoded_preferred_size(&edid), size);
            assert_eq!(edid.as_bytes()[128 + 4], 0x41);
            assert_eq!(edid.as_bytes()[128 + 5], vic | 0x80);
        }
    }

    #[test]
    fn unmanaged_mode_has_no_edid_image() {
        assert!(EdidImage::for_mode(VirtualMonitorMode::Unmanaged).is_none());
    }
}
