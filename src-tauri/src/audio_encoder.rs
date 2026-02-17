//! WAV encoder for PCM audio data.
//!
//! Converts raw f32 PCM samples to a WAV byte buffer.
//! Output format: 16-bit signed integer PCM, suitable for the OpenAI Whisper API.

/// Encode raw f32 PCM samples into a WAV byte buffer (16-bit PCM format).
///
/// Converts each f32 sample (expected range [-1.0, 1.0]) to i16 by clamping and scaling.
/// The resulting WAV data is written to an in-memory buffer and returned as `Vec<u8>`.
pub fn encode_pcm_to_wav(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> anyhow::Result<Vec<u8>> {
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits_per_sample) / 8;
    let block_align = channels * bits_per_sample / 8;
    #[allow(clippy::cast_possible_truncation)]
    let data_size = (samples.len() as u32) * u32::from(bits_per_sample) / 8;
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        #[allow(clippy::cast_possible_truncation)]
        let sample_i16 = (clamped * f32::from(i16::MAX)) as i16;
        buf.extend_from_slice(&sample_i16.to_le_bytes());
    }

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty_samples_produces_valid_wav() {
        let result = encode_pcm_to_wav(&[], 16000, 1);
        assert!(result.is_ok());
        let wav_bytes = result.unwrap();
        assert!(wav_bytes.len() >= 44);
        assert_eq!(&wav_bytes[0..4], b"RIFF");
        assert_eq!(&wav_bytes[8..12], b"WAVE");
    }

    #[test]
    fn encode_samples_produces_correct_data_size() {
        let samples = vec![0.0_f32; 100];
        let result = encode_pcm_to_wav(&samples, 44100, 1);
        assert!(result.is_ok());
        let wav_bytes = result.unwrap();
        // 44 bytes header + 100 samples * 2 bytes per sample (16-bit) = 244
        assert_eq!(wav_bytes.len(), 44 + 100 * 2);
    }

    #[test]
    fn encode_clamps_out_of_range_samples() {
        let samples = vec![-2.0_f32, 2.0, 0.5];
        let result = encode_pcm_to_wav(&samples, 16000, 1);
        assert!(result.is_ok());
    }
}
