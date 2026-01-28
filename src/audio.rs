use anyhow::{Context, Result};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Represents loaded audio data
#[derive(Clone)]
pub struct AudioData {
    /// Duration in seconds
    pub duration: f64,
    /// File name
    pub name: String,
    /// Original sample rate
    pub sample_rate: u32,
    /// Samples normalized to -1.0 to 1.0 range (mono, downsampled for waveform)
    pub samples: Vec<f32>,
}

impl AudioData {
    /// Load audio from a file path
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("audio")
            .to_string();

        let file = File::open(path).context("Failed to open audio file")?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let format_opts = FormatOptions::default();
        let metadata_opts = MetadataOptions::default();
        let decoder_opts = DecoderOptions::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)
            .context("Failed to probe audio format")?;

        let mut format = probed.format;

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
            .context("No audio track found")?;

        let track_id = track.id;
        let sample_rate = track
            .codec_params
            .sample_rate
            .context("No sample rate")?;
        let channels = track
            .codec_params
            .channels
            .map(|c| c.count())
            .unwrap_or(2);

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &decoder_opts)
            .context("Failed to create decoder")?;

        let mut all_samples: Vec<f32> = Vec::new();

        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(symphonia::core::errors::Error::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(e) => return Err(e.into()),
            };

            if packet.track_id() != track_id {
                continue;
            }

            let decoded = match decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(_) => continue,
            };

            let spec = *decoded.spec();
            let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            sample_buf.copy_interleaved_ref(decoded);

            let samples = sample_buf.samples();
            
            // Convert to mono by averaging channels
            for chunk in samples.chunks(channels) {
                let mono: f32 = chunk.iter().sum::<f32>() / channels as f32;
                all_samples.push(mono);
            }
        }

        let duration = all_samples.len() as f64 / sample_rate as f64;

        // Downsample for waveform display (target ~4000 samples for visualization)
        let target_samples = 4000;
        let samples = if all_samples.len() > target_samples {
            let chunk_size = all_samples.len() / target_samples;
            all_samples
                .chunks(chunk_size)
                .map(|chunk| {
                    // Use peak value for better waveform visualization
                    chunk
                        .iter()
                        .map(|s| s.abs())
                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                        .unwrap_or(0.0)
                })
                .collect()
        } else {
            all_samples.iter().map(|s| s.abs()).collect()
        };

        Ok(Self {
            samples,
            sample_rate,
            duration,
            name,
        })
    }
}
