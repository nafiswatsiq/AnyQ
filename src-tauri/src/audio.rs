use base64::{Engine as _, engine::general_purpose::STANDARD};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::{
    collections::VecDeque,
    io::Cursor,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use crate::config::AudioSource;

pub fn record(
    device_name: Option<&str>,
    source: &AudioSource,
    stop: Arc<std::sync::atomic::AtomicBool>,
) -> Result<String, String> {
    let (samples, sample_rate, channels) = match source {
        AudioSource::Microphone => record_microphone(device_name, &stop)?,
        AudioSource::SystemAudio => record_system_audio(&stop)?,
    };
    encode_wav(samples, sample_rate, channels)
}

fn record_microphone(
    device_name: Option<&str>,
    stop: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<(Vec<f32>, u32, usize), String> {
    let host = cpal::default_host();
    let device = if let Some(name) = device_name {
        host.input_devices()
            .map_err(|error| error.to_string())?
            .find(|device| {
                device
                    .description()
                    .map(|description| description.name() == name)
                    .unwrap_or(false)
            })
            .ok_or_else(|| format!("Microphone not found: {name}"))?
    } else {
        host.default_input_device()
            .ok_or_else(|| "No default microphone found".to_string())?
    };
    let supported = device
        .default_input_config()
        .map_err(|error| error.to_string())?;
    let sample_rate = supported.sample_rate();
    let channels = supported.channels() as usize;
    let (sample_sender, sample_receiver) = mpsc::channel::<Vec<f32>>();
    let error_callback = |error| eprintln!("Audio input error: {error}");
    let stream_config = supported.clone().into();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            let sender = sample_sender.clone();
            device.build_input_stream(
                stream_config,
                move |data: &[f32], _| {
                    let _ = sender.send(data.to_vec());
                },
                error_callback,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let sender = sample_sender.clone();
            device.build_input_stream(
                stream_config,
                move |data: &[i16], _| {
                    let samples = data
                        .iter()
                        .map(|sample| *sample as f32 / i16::MAX as f32)
                        .collect();
                    let _ = sender.send(samples);
                },
                error_callback,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let sender = sample_sender.clone();
            device.build_input_stream(
                stream_config,
                move |data: &[u16], _| {
                    let samples = data
                        .iter()
                        .map(|sample| *sample as f32 / u16::MAX as f32 * 2.0 - 1.0)
                        .collect();
                    let _ = sender.send(samples);
                },
                error_callback,
                None,
            )
        }
        format => return Err(format!("Unsupported microphone sample format: {format:?}")),
    }
    .map_err(|error| error.to_string())?;
    drop(sample_sender);
    stream.play().map_err(|error| error.to_string())?;

    let mut samples = Vec::<f32>::new();
    let started = Instant::now();
    while stop.load(std::sync::atomic::Ordering::SeqCst)
        && started.elapsed() < Duration::from_secs(60)
    {
        while let Ok(chunk) = sample_receiver.try_recv() {
            samples.extend(chunk);
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    while let Ok(chunk) = sample_receiver.try_recv() {
        samples.extend(chunk);
    }
    drop(stream);

    let mono = samples
        .chunks(channels)
        .map(|chunk| chunk.iter().copied().sum::<f32>() / chunk.len() as f32)
        .collect::<Vec<_>>();
    if mono.is_empty() {
        return Err("No microphone samples were captured".to_string());
    }
    let target_count = ((mono.len() as u64 * 16_000) / u64::from(sample_rate)) as usize;
    let mut output = Vec::with_capacity(target_count);
    for index in 0..target_count {
        let source = index as f64 * f64::from(sample_rate) / 16_000.0;
        let left = source.floor() as usize;
        let right = (left + 1).min(mono.len().saturating_sub(1));
        let fraction = source.fract() as f32;
        output.push(mono[left] * (1.0 - fraction) + mono[right] * fraction);
    }

    Ok((output, 16_000, 1))
}

#[cfg(target_os = "windows")]
fn record_system_audio(
    stop: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<(Vec<f32>, u32, usize), String> {
    use wasapi::{
        initialize_mta, AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode,
        WaveFormat,
    };

    let hresult = initialize_mta();
    if hresult.is_err() {
        return Err(format!("Failed to initialize WASAPI COM: {hresult:?}"));
    }
    let enumerator = DeviceEnumerator::new().map_err(|error| error.to_string())?;
    let device = enumerator
        .get_default_device(&Direction::Render)
        .map_err(|error| error.to_string())?;
    let mut client: AudioClient = device
        .get_iaudioclient()
        .map_err(|error| error.to_string())?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
    let (_, minimum_period) = client
        .get_device_period()
        .map_err(|error| error.to_string())?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: minimum_period,
    };
    client
        .initialize_client(&format, &Direction::Capture, &mode)
        .map_err(|error| error.to_string())?;
    let event = client.set_get_eventhandle().map_err(|error| error.to_string())?;
    let capture = client
        .get_audiocaptureclient()
        .map_err(|error| error.to_string())?;
    let mut bytes = VecDeque::new();
    client.start_stream().map_err(|error| error.to_string())?;

    let started = Instant::now();
    while stop.load(std::sync::atomic::Ordering::SeqCst)
        && started.elapsed() < Duration::from_secs(60)
    {
        capture
            .read_from_device_to_deque(&mut bytes)
            .map_err(|error| error.to_string())?;
        event
            .wait_for_event(100)
            .map_err(|error| error.to_string())?;
    }
    client.stop_stream().map_err(|error| error.to_string())?;

    let raw = bytes.into_iter().collect::<Vec<_>>();
    let samples = raw
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect::<Vec<_>>();
    if samples.is_empty() {
        return Err("No system audio samples were captured".to_string());
    }
    Ok((samples, 48_000, 2))
}

#[cfg(not(target_os = "windows"))]
fn record_system_audio(
    _stop: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<(Vec<f32>, u32, usize), String> {
    Err("System audio capture is only supported on Windows".to_string())
}

fn encode_wav(
    samples: Vec<f32>,
    sample_rate: u32,
    channels: usize,
) -> Result<String, String> {
    let mono = samples
        .chunks(channels)
        .map(|chunk| chunk.iter().copied().sum::<f32>() / chunk.len() as f32)
        .collect::<Vec<_>>();
    let target_count = ((mono.len() as u64 * 16_000) / u64::from(sample_rate)) as usize;
    let mut output = Vec::with_capacity(target_count);
    for index in 0..target_count {
        let source = index as f64 * f64::from(sample_rate) / 16_000.0;
        let left = source.floor() as usize;
        let right = (left + 1).min(mono.len().saturating_sub(1));
        let fraction = source.fract() as f32;
        output.push(mono[left] * (1.0 - fraction) + mono[right] * fraction);
    }
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut bytes = Cursor::new(Vec::new());
    let mut writer = WavWriter::new(&mut bytes, spec).map_err(|error| error.to_string())?;
    for sample in output {
        writer
            .write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .map_err(|error| error.to_string())?;
    }
    writer.finalize().map_err(|error| error.to_string())?;
    Ok(STANDARD.encode(bytes.into_inner()))
}
