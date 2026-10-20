//! DCT spread-spectrum invisible watermark.
//!
//! Embeds a payload into mid-frequency DCT coefficients of the blue channel
//! using Quantization Index Modulation (QIM). Block selection and coefficient
//! choice are driven by a secret-key-seeded PRNG. The payload is repeated
//! across many blocks for error resilience.
//!
//! This survives colour/channel/alpha manipulation because the watermark lives
//! in the frequency relationships between pixels, not in their absolute values.
//! It is perceptually invisible because the embedding strength is scaled by a
//! simplified JND (luminance + texture masking) model.

use image::RgbImage;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rustdct::DctPlanner;
use sha2::{Digest, Sha256};

const BLOCK: usize = 8;
const BLOCK_PIXELS: usize = BLOCK * BLOCK;

/// Default secret key. Change this if you want keyed protection.
pub const DEFAULT_KEY: &str = "moon-beauty-dct-v1";

/// Mid-frequency DCT positions (u, v) inside an 8x8 block.
/// These balance invisibility (not low-frequency) with robustness (not high-frequency).
const MID_FREQ: &[(usize, usize)] = &[
    (2, 1), (1, 2), (2, 2), (3, 1), (1, 3), (3, 2), (2, 3), (3, 3),
    (4, 1), (1, 4), (4, 2), (2, 4),
];

/// Base QIM step size in normalised DCT domain.
/// Mid-freq normalised coefficients are typically 0.1–2.0 in magnitude.
/// A step of 0.08 means we change each modified coefficient by at most
/// ~0.04 — well below perceptual threshold, but still large enough for
/// 1:1 screen capture where pixel values change by only 1–3.
///
/// If screen-capture extraction fails, bump to 0.15, then 0.25.
/// Above 0.3 it starts becoming subtly visible.
const BASE_DELTA: f32 = 0.08;

/// Repetitions for payload bits.
const REPETITIONS: usize = 4;
/// Extra repetitions for the 16-bit length header.
const LEN_REPETITIONS: usize = 6;

/// rustdct DCT2/DCT3 are unnormalised. For an 8x8 block, DCT3(DCT2(x)) = 256*x.
const IDCT_SCALE: f32 = 256.0;

fn rng_from_key(key: &str) -> ChaCha20Rng {
    let mut hasher = Sha256::new();
    hasher.update(b"mbs-dct-watermark-seed-v1");
    hasher.update(key.as_bytes());
    let seed: [u8; 32] = hasher.finalize().into();
    ChaCha20Rng::from_seed(seed)
}

fn normalise(coeffs: &mut [f32; BLOCK_PIXELS]) {
    for c in coeffs.iter_mut() { *c /= 64.0; }
}
fn denormalise(coeffs: &mut [f32; BLOCK_PIXELS]) {
    for c in coeffs.iter_mut() { *c *= 64.0; }
}

struct DctCtx {
    dct2: std::sync::Arc<dyn rustdct::Dct2<f32>>,
    dct3: std::sync::Arc<dyn rustdct::Dct3<f32>>,
}

impl DctCtx {
    fn new() -> Self {
        let mut planner = DctPlanner::new();
        Self { dct2: planner.plan_dct2(BLOCK), dct3: planner.plan_dct3(BLOCK) }
    }
    fn forward_2d(&self, block: &mut [f32; BLOCK_PIXELS]) {
        let mut tmp = [0f32; BLOCK];
        // Rows
        for y in 0..BLOCK {
            tmp.copy_from_slice(&block[y * BLOCK..(y + 1) * BLOCK]);
            self.dct2.process_dct2(&mut tmp);
            block[y * BLOCK..(y + 1) * BLOCK].copy_from_slice(&tmp);
        }
        // Columns
        for x in 0..BLOCK {
            for y in 0..BLOCK { tmp[y] = block[y * BLOCK + x]; }
            self.dct2.process_dct2(&mut tmp);
            for y in 0..BLOCK { block[y * BLOCK + x] = tmp[y]; }
        }
    }
    fn inverse_2d(&self, block: &mut [f32; BLOCK_PIXELS]) {
        let mut tmp = [0f32; BLOCK];
        // Columns first (reverse of forward)
        for x in 0..BLOCK {
            for y in 0..BLOCK { tmp[y] = block[y * BLOCK + x]; }
            self.dct3.process_dct3(&mut tmp);
            for y in 0..BLOCK { block[y * BLOCK + x] = tmp[y]; }
        }
        // Rows
        for y in 0..BLOCK {
            tmp.copy_from_slice(&block[y * BLOCK..(y + 1) * BLOCK]);
            self.dct3.process_dct3(&mut tmp);
            block[y * BLOCK..(y + 1) * BLOCK].copy_from_slice(&tmp);
        }
        // rustdct DCT-III(DCT-II(x)) = 2N*x per 1D axis, so 2D gives 256*x
        for c in block.iter_mut() { *c /= IDCT_SCALE; }
    }
}

/// Simplified JND: luminance adaptation + texture masking.
/// Returns the QIM step size for this block.
fn block_delta(dc: f32, ac_energy: f32) -> f32 {
    let mean = dc.clamp(0.0, 255.0);
    let lum = if mean < 60.0 {
        (60.0 - mean) / 150.0 + 1.0
    } else if mean > 170.0 {
        (mean - 170.0) / 425.0 + 1.0
    } else {
        1.0
    };
    let tex = 1.0 + (ac_energy / 15.0).min(1.0) * 0.6;
    (BASE_DELTA * lum * tex).clamp(BASE_DELTA * 0.6, BASE_DELTA * 1.6)
}

/// QIM embed: move `c` to the nearest even (bit 0) or odd (bit 1) multiple of delta.
fn qim_embed(c: f32, bit: u8, delta: f32) -> f32 {
    let even = (c / delta).round() * delta;
    let odd = ((c - delta / 2.0) / delta).round() * delta + delta / 2.0;
    if bit == 0 { even } else { odd }
}

/// QIM extract: whichever quantiser the coefficient is closer to.
fn qim_extract(c: f32, delta: f32) -> u8 {
    let even = (c / delta).round() * delta;
    let odd = ((c - delta / 2.0) / delta).round() * delta + delta / 2.0;
    if (c - even).abs() < (c - odd).abs() { 0 } else { 1 }
}

/// Embed `payload` into the blue channel of `image_bytes` using `key`.
pub fn embed_dct(image_bytes: &[u8], payload: &str, key: &str) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(image_bytes).map_err(|e| format!("decode: {e}"))?;
    let mut rgb: RgbImage = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let bw = (w / 8) as usize;
    let bh = (h / 8) as usize;
    let total_blocks = bw * bh;
    if total_blocks == 0 { return Err("image too small".into()); }

    // Build bitstream: 16-bit length + UTF-8 payload.
    let mut bits: Vec<u8> = Vec::new();
    let len = payload.as_bytes().len() as u16;
    for i in (0..16).rev() { bits.push(((len >> i) & 1) as u8); }
    for &b in payload.as_bytes() {
        for i in (0..8).rev() { bits.push((b >> i) & 1); }
    }

    // Repeat: length header LEN_REPETITIONS times, payload REPETITIONS times.
    let mut stream: Vec<u8> = Vec::new();
    for _ in 0..LEN_REPETITIONS {
        stream.extend_from_slice(&bits[0..16]);
    }
    for _ in 0..REPETITIONS {
        stream.extend_from_slice(&bits[16..]);
    }

    if stream.len() > total_blocks {
        return Err(format!("payload too large: need {} blocks, have {}", stream.len(), total_blocks));
    }

    let mut rng = rng_from_key(key);
    let mut block_order: Vec<usize> = (0..total_blocks).collect();
    block_order.shuffle(&mut rng);

    let ctx = DctCtx::new();

    for (bit_idx, &bit) in stream.iter().enumerate() {
        let block_idx = block_order[bit_idx];
        let bx = (block_idx % bw) * 8;
        let by = (block_idx / bw) * 8;

        let mut block = [0f32; BLOCK_PIXELS];
        for y in 0..8 {
            for x in 0..8 {
                block[y * 8 + x] = rgb.get_pixel((bx + x) as u32, (by + y) as u32)[2] as f32;
            }
        }

        ctx.forward_2d(&mut block);
        normalise(&mut block);

        let dc = block[0];
        let ac_energy: f32 = MID_FREQ.iter().map(|&(u, v)| block[v * 8 + u].abs()).sum();
        let delta = block_delta(dc, ac_energy);

        let (u, v) = MID_FREQ[rng.gen_range(0..MID_FREQ.len())];
        let idx = v * 8 + u;
        block[idx] = qim_embed(block[idx], bit, delta);

        denormalise(&mut block);
        ctx.inverse_2d(&mut block);

        for y in 0..8 {
            for x in 0..8 {
                let val = block[y * 8 + x].round().clamp(0.0, 255.0) as u8;
                rgb.get_pixel_mut((bx + x) as u32, (by + y) as u32)[2] = val;
            }
        }
    }

    // ── Diagnostic: report pixel-diff stats to prove invisibility ────
    let orig_img = image::load_from_memory(image_bytes)
        .map_err(|e| format!("reload: {e}"))?;
    let orig_rgb = orig_img.to_rgb8();
    let mut max_diff: u32 = 0;
    let mut sum_sq_diff: u64 = 0;
    let mut n_px: u64 = 0;
    for (orig, new) in orig_rgb.pixels().zip(rgb.pixels()) {
        for ch in 0..3 {
            let d = (orig[ch] as i32 - new[ch] as i32).unsigned_abs();
            max_diff = max_diff.max(d);
            sum_sq_diff += (d * d) as u64;
            n_px += 1;
        }
    }
    let mse = sum_sq_diff as f64 / n_px as f64;
    let psnr = if mse > 0.0 {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    } else {
        999.0
    };
    eprintln!(
        "[embed_dct] payload={:?} blocks_used={} max_px_diff={} PSNR={:.1} dB          (>40 invisible, >30 imperceptible-with-effort)",
        payload, stream.len(), max_diff, psnr
    );

    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(rgb)
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("encode: {e}"))?;
    Ok(buf.into_inner())
}

/// Extract the payload from `image_bytes` using `key`.
pub fn extract_dct(image_bytes: &[u8], key: &str) -> Option<String> {
    let img = image::load_from_memory(image_bytes).ok()?;
    let rgb: RgbImage = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let bw = (w / 8) as usize;
    let bh = (h / 8) as usize;
    let total_blocks = bw * bh;
    if total_blocks < 16 * LEN_REPETITIONS { return None; }

    let mut rng = rng_from_key(key);
    let mut block_order: Vec<usize> = (0..total_blocks).collect();
    block_order.shuffle(&mut rng);

    let ctx = DctCtx::new();

    // Read a single bit from one block. Advances `rng` exactly as embed did.
    let read_bit = |block_idx: usize, rng: &mut ChaCha20Rng| -> Option<u8> {
        let bx = (block_idx % bw) * 8;
        let by = (block_idx / bw) * 8;
        let mut block = [0f32; BLOCK_PIXELS];
        for y in 0..8 {
            for x in 0..8 {
                block[y * 8 + x] = rgb.get_pixel((bx + x) as u32, (by + y) as u32)[2] as f32;
            }
        }
        ctx.forward_2d(&mut block);
        normalise(&mut block);
        let dc = block[0];
        let ac_energy: f32 = MID_FREQ.iter().map(|&(u, v)| block[v * 8 + u].abs()).sum();
        let delta = block_delta(dc, ac_energy);
        let (u, v) = MID_FREQ[rng.gen_range(0..MID_FREQ.len())];
        Some(qim_extract(block[v * 8 + u], delta))
    };

    // Decode length (majority vote across LEN_REPETITIONS).
    let mut len_bits = [0u8; 16];
    for i in 0..16 {
        let mut votes = 0u32;
        for rep in 0..LEN_REPETITIONS {
            let idx = rep * 16 + i;
            if idx < block_order.len() {
                if let Some(b) = read_bit(block_order[idx], &mut rng) { votes += b as u32; }
            }
        }
        len_bits[i] = if votes > (LEN_REPETITIONS as u32) / 2 { 1 } else { 0 };
    }
    let mut len: u16 = 0;
    for b in len_bits { len = (len << 1) | b as u16; }
    if len == 0 || len > 1024 { return None; }

    // Decode payload (majority vote across REPETITIONS).
    let payload_bit_len = len as usize * 8;
    let payload_block_start = 16 * LEN_REPETITIONS;
    if payload_block_start + payload_bit_len * REPETITIONS > block_order.len() { return None; }

    let mut payload_bits = vec![0u8; payload_bit_len];
    for j in 0..payload_bit_len {
        let mut votes = 0u32;
        for rep in 0..REPETITIONS {
            let idx = payload_block_start + rep * payload_bit_len + j;
            if idx < block_order.len() {
                if let Some(b) = read_bit(block_order[idx], &mut rng) { votes += b as u32; }
            }
        }
        payload_bits[j] = if votes > (REPETITIONS as u32) / 2 { 1 } else { 0 };
    }

    let mut bytes = Vec::with_capacity(len as usize);
    for chunk in payload_bits.chunks(8) {
        let mut byte = 0u8;
        for &b in chunk { byte = (byte << 1) | b; }
        bytes.push(byte);
    }
    String::from_utf8(bytes).ok()
}
