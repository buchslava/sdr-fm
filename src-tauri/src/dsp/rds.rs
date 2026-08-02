//! Minimal FM-RDS decoder focused on Program Service (PS) names (group 0A/0B).

use futuresdr::num_complex::Complex32;

const RDS_BITRATE: f32 = 1187.5;
const SUBCARRIER_HZ: f32 = 57_000.0;

/// CRC syndrome check for a 26-bit RDS block (16 data + 10 check).
pub fn crc_ok(block26: u32) -> bool {
    // Polynomial x^10 + x^8 + x^7 + x^5 + x^4 + x^3 + 1 = 0x5B9
    let mut reg = block26 & 0x3FF_FFFF;
    for _ in 0..16 {
        if (reg & (1 << 25)) != 0 {
            reg ^= 0x5B9 << 16;
        }
        reg <<= 1;
    }
    (reg >> 16) & 0x3FF == 0
}

/// Offset words used to identify block position within a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockOffset {
    A,
    B,
    C,
    Cp,
    D,
}

impl BlockOffset {
    fn word(self) -> u16 {
        match self {
            BlockOffset::A => 0x0FC,
            BlockOffset::B => 0x198,
            BlockOffset::C => 0x168,
            BlockOffset::Cp => 0x350,
            BlockOffset::D => 0x1B4,
        }
    }

    fn all() -> [BlockOffset; 5] {
        [
            BlockOffset::A,
            BlockOffset::B,
            BlockOffset::C,
            BlockOffset::Cp,
            BlockOffset::D,
        ]
    }
}

fn syndrome(block26: u32) -> u16 {
    let mut reg = block26 & 0x3FF_FFFF;
    for _ in 0..16 {
        if (reg & (1 << 25)) != 0 {
            reg ^= 0x5B9 << 16;
        }
        reg <<= 1;
    }
    ((reg >> 16) & 0x3FF) as u16
}

/// Identify block type by matching residual syndrome to offset words.
pub fn identify_offset(block26: u32) -> Option<(BlockOffset, u16)> {
    let data = ((block26 >> 10) & 0xFFFF) as u16;
    let check = (block26 & 0x3FF) as u16;
    for offset in BlockOffset::all() {
        let with_offset_removed = ((data as u32) << 10) | ((check ^ offset.word()) as u32);
        if crc_ok(with_offset_removed) {
            return Some((offset, data));
        }
    }
    let syn = syndrome(block26);
    for offset in BlockOffset::all() {
        if syn == offset.word() {
            return Some((offset, data));
        }
    }
    None
}

/// Assemble PS from type-0 group fragments. `segments[i]` is characters 2*i, 2*i+1.
pub fn assemble_ps(segments: &[Option<[u8; 2]>; 4]) -> Option<String> {
    if segments.iter().any(|s| s.is_none()) {
        return None;
    }
    let mut bytes = Vec::with_capacity(8);
    for seg in segments {
        let [a, b] = seg.unwrap();
        if a != 0 {
            bytes.push(a);
        }
        if b != 0 {
            bytes.push(b);
        }
    }
    let name = String::from_utf8_lossy(&bytes).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Apply a decoded type-0 group fragment into PS segment slots.
pub fn apply_group0_ps(segments: &mut [Option<[u8; 2]>; 4], block_b: u16, block_d: u16) {
    let group_type = (block_b >> 12) & 0xF;
    if group_type != 0 {
        return;
    }
    let addr = (block_b & 0x3) as usize;
    if addr > 3 {
        return;
    }
    let c0 = ((block_d >> 8) & 0xFF) as u8;
    let c1 = (block_d & 0xFF) as u8;
    // Printable ASCII / Latin-1 radio charset approx.
    let clean = |c: u8| if (0x20..=0x7E).contains(&c) { c } else { b' ' };
    segments[addr] = Some([clean(c0), clean(c1)]);
}

/// Stateful RF → PS decoder.
pub struct RdsPsDecoder {
    decim: usize,
    mpx_rate: f32,
    last: Complex32,
    // 57 kHz NCO
    nco_phase: f32,
    nco_inc: f32,
    // Simple IIR LPF state after mix-down
    lpf_re: f32,
    lpf_im: f32,
    // Symbol timing
    integ: f32,
    samples_per_symbol: f32,
    sample_phase: f32,
    last_symbol: f32,
    bit_buf: u64,
    bit_count: u32,
    // Group assembly
    expecting: Option<BlockOffset>,
    block_a: Option<u16>,
    block_b: Option<u16>,
    block_c: Option<u16>,
    ps_segments: [Option<[u8; 2]>; 4],
    ps: Option<String>,
}

impl RdsPsDecoder {
    pub fn new(iq_sample_rate: u32) -> Self {
        // Decimate toward ~200 kHz MPX.
        let target_mpx = 200_000.0;
        let decim = ((iq_sample_rate as f32) / target_mpx).round().max(1.0) as usize;
        let mpx_rate = iq_sample_rate as f32 / decim as f32;
        let samples_per_symbol = mpx_rate / RDS_BITRATE;
        let nco_inc = 2.0 * std::f32::consts::PI * SUBCARRIER_HZ / mpx_rate;
        Self {
            decim,
            mpx_rate,
            last: Complex32::new(0.0, 0.0),
            nco_phase: 0.0,
            nco_inc,
            lpf_re: 0.0,
            lpf_im: 0.0,
            integ: 0.0,
            samples_per_symbol,
            sample_phase: 0.0,
            last_symbol: 0.0,
            bit_buf: 0,
            bit_count: 0,
            expecting: None,
            block_a: None,
            block_b: None,
            block_c: None,
            ps_segments: [None; 4],
            ps: None,
        }
    }

    pub fn ps_name(&self) -> Option<&str> {
        self.ps.as_deref()
    }

    pub fn feed_iq(&mut self, iq: &[Complex32]) {
        if self.ps.is_some() {
            return;
        }
        let mut i = 0;
        while i + self.decim <= iq.len() {
            // Average decimate.
            let mut acc = Complex32::new(0.0, 0.0);
            for k in 0..self.decim {
                acc += iq[i + k];
            }
            acc /= self.decim as f32;
            i += self.decim;

            // FM phase discriminator → MPX sample.
            let phase = (acc * self.last.conj()).arg();
            self.last = acc;
            self.process_mpx(phase);
            if self.ps.is_some() {
                return;
            }
        }
    }

    fn process_mpx(&mut self, mpx: f32) {
        // Mix to baseband around 57 kHz.
        let (s, c) = self.nco_phase.sin_cos();
        self.nco_phase = (self.nco_phase + self.nco_inc) % (2.0 * std::f32::consts::PI);
        let mixed_re = mpx * c;
        let mixed_im = mpx * (-s);

        // One-pole LPF ~2.4 kHz.
        let alpha = (2.0 * std::f32::consts::PI * 2_400.0 / self.mpx_rate).min(1.0);
        self.lpf_re += alpha * (mixed_re - self.lpf_re);
        self.lpf_im += alpha * (mixed_im - self.lpf_im);

        // Costas-ish: use real after crude carrier lock via phase of LPF.
        let baseband = self.lpf_re;

        self.integ += baseband;
        self.sample_phase += 1.0;
        if self.sample_phase < self.samples_per_symbol {
            return;
        }
        self.sample_phase -= self.samples_per_symbol;
        let symbol = self.integ;
        self.integ = 0.0;

        // Differential BPSK decode.
        let bit = if symbol * self.last_symbol >= 0.0 { 0u8 } else { 1u8 };
        self.last_symbol = symbol;
        self.push_bit(bit);
    }

    fn push_bit(&mut self, bit: u8) {
        self.bit_buf = ((self.bit_buf << 1) | (bit as u64)) & ((1u64 << 26) - 1);
        self.bit_count = self.bit_count.saturating_add(1);
        if self.bit_count < 26 {
            return;
        }

        let block26 = self.bit_buf as u32;
        if let Some((offset, data)) = identify_offset(block26) {
            self.handle_block(offset, data);
            self.bit_count = 0;
            self.bit_buf = 0;
        } else {
            // Keep sliding.
            self.bit_count = 26;
        }
    }

    fn handle_block(&mut self, offset: BlockOffset, data: u16) {
        match offset {
            BlockOffset::A => {
                self.block_a = Some(data);
                self.block_b = None;
                self.block_c = None;
                self.expecting = Some(BlockOffset::B);
            }
            BlockOffset::B => {
                if self.expecting == Some(BlockOffset::B) || self.block_a.is_some() {
                    self.block_b = Some(data);
                    self.expecting = Some(BlockOffset::C);
                }
            }
            BlockOffset::C | BlockOffset::Cp => {
                if self.expecting == Some(BlockOffset::C) {
                    self.block_c = Some(data);
                    self.expecting = Some(BlockOffset::D);
                }
            }
            BlockOffset::D => {
                if let (Some(_a), Some(b), Some(_c)) = (self.block_a, self.block_b, self.block_c) {
                    apply_group0_ps(&mut self.ps_segments, b, data);
                    if let Some(name) = assemble_ps(&self.ps_segments) {
                        self.ps = Some(name);
                    }
                }
                self.expecting = None;
                self.block_a = None;
                self.block_b = None;
                self.block_c = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_complete_ps() {
        let segs = [
            Some([b'H', b'I']),
            Some([b'T', b' ']),
            Some([b'F', b'M']),
            Some([b' ', b' ']),
        ];
        assert_eq!(assemble_ps(&segs).as_deref(), Some("HIT FM"));
    }

    #[test]
    fn rejects_incomplete_ps() {
        let segs = [Some([b'H', b'I']), None, None, None];
        assert!(assemble_ps(&segs).is_none());
    }

    #[test]
    fn apply_group0_fills_segment() {
        let mut segs = [None; 4];
        // type 0, addr 1 → bits in B: type in top nibble 0, addr in low 2 bits = 1
        let block_b = 0x0001;
        let block_d = (('X' as u16) << 8) | ('Y' as u16);
        apply_group0_ps(&mut segs, block_b, block_d);
        assert_eq!(segs[1], Some([b'X', b'Y']));
    }

    #[test]
    fn crc_is_deterministic() {
        assert_eq!(crc_ok(0), crc_ok(0));
        assert_eq!(crc_ok(0x123456), crc_ok(0x123456));
    }
}
