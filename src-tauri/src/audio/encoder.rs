use anyhow::{bail, Result};
use bytes::BufMut;
use opus::{Application, Bitrate, Channels, Encoder};

use super::SAMPLE_RATE;

pub struct OpusEnc {
    enc: Encoder,
    pub frame_frames: usize,
}

impl OpusEnc {
    pub fn new(frame_ms: f32, bitrate_kbps: u32) -> Result<Self> {
        let frame_frames = (frame_ms * SAMPLE_RATE as f32 / 1000.0).round() as usize;
        if frame_frames != 120 && frame_frames != 240 {
            bail!("taille de trame Opus non supportée : {frame_ms} ms (2.5 ou 5)");
        }
        let mut enc = Encoder::new(SAMPLE_RATE, Channels::Stereo, Application::LowDelay)?;
        enc.set_bitrate(Bitrate::Bits((bitrate_kbps * 1000) as i32))?;
        enc.set_vbr(false)?;
        Ok(Self { enc, frame_frames })
    }

    pub fn encode(&mut self, pcm: &[f32], out: &mut [u8]) -> Result<usize> {
        debug_assert_eq!(pcm.len(), self.frame_frames * 2);
        Ok(self.enc.encode_float(pcm, out)?)
    }
}

pub fn wav_streaming_header(sample_rate: u32, channels: u16) -> [u8; 44] {
    let bits = 16u16;
    let block_align = channels * bits / 8;
    let byte_rate = sample_rate * block_align as u32;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&bits.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    h
}

pub fn f32_to_s16le(src: &[f32], dst: &mut impl BufMut) {
    for &x in src {
        dst.put_i16_le((x.clamp(-1.0, 1.0) * 32767.0).round() as i16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_layout() {
        let h = wav_streaming_header(48000, 2);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(&h[36..40], b"data");
        assert_eq!(u32::from_le_bytes(h[24..28].try_into().unwrap()), 48000);
        assert_eq!(u32::from_le_bytes(h[28..32].try_into().unwrap()), 192000);
    }

    #[test]
    fn s16_conversion_clamps() {
        let mut v = Vec::new();
        f32_to_s16le(&[2.0, -2.0, 0.0], &mut v);
        assert_eq!(i16::from_le_bytes([v[0], v[1]]), 32767);
        assert_eq!(i16::from_le_bytes([v[2], v[3]]), -32767);
        assert_eq!(i16::from_le_bytes([v[4], v[5]]), 0);
    }
}
