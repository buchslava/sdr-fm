use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Stream, StreamConfig};
use futuresdr::prelude::*;
use futuresdr::runtime::dev::prelude::*;

use super::linux_audio::OutputSetup;

const QUEUE_SIZE: usize = 5;

/// Linux audio sink with explicit ALSA device selection (Pi-friendly).
#[derive(Block)]
pub struct LinuxAudioSink<I = DefaultCpuReader<f32>>
where
    I: CpuBufferReader<Item = f32>,
{
    #[input]
    input: I,
    setup: OutputSetup,
    stream: Option<Stream>,
    min_buffer_size: usize,
    vec: Vec<f32>,
    terminated: Option<oneshot::Receiver<()>>,
    tx: Option<mpsc::Sender<Vec<f32>>>,
}

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<I> Send for LinuxAudioSink<I> where I: CpuBufferReader<Item = f32> {}

impl LinuxAudioSink<DefaultCpuReader<f32>> {
    pub fn new(setup: OutputSetup) -> Result<Self, String> {
        Ok(Self {
            input: DefaultCpuReader::default(),
            setup,
            stream: None,
            min_buffer_size: 1024,
            vec: Vec::new(),
            terminated: None,
            tx: None,
        })
    }
}

#[doc(hidden)]
impl<I> Kernel for LinuxAudioSink<I>
where
    I: CpuBufferReader<Item = f32>,
{
    async fn init(&mut self, _mo: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .devices()
            .map_err(|e| Error::RuntimeError(e.to_string()))?
            .find(|d| d.name().ok().as_deref() == Some(self.setup.pcm_id()))
            .ok_or_else(|| {
                Error::RuntimeError(format!(
                    "Audio device {} is no longer available",
                    self.setup.pcm_id()
                ))
            })?;

        let config = StreamConfig {
            channels: self.setup.output_channels,
            sample_rate: self.setup.sample_rate,
            buffer_size: BufferSize::Default,
        };

        let duplicate_mono = self.setup.duplicate_mono;
        let (terminate, terminated) = oneshot::channel();
        let mut terminate = Some(terminate);
        self.terminated = Some(terminated);
        let (tx, rx) = mpsc::channel(QUEUE_SIZE);
        let mut iter: Option<Vec<f32>> = None;

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut i = 0;
                    while let Some(mut v) = iter.take().or_else(|| rx.try_recv().ok()) {
                        if v.is_empty() {
                            if let Some(t) = terminate.take() {
                                let _ = t.send(());
                            }
                            return;
                        }
                        if duplicate_mono {
                            let n = std::cmp::min(v.len(), (data.len() - i) / 2);
                            for (j, sample) in v.iter().take(n).enumerate() {
                                data[i + 2 * j] = *sample;
                                data[i + 2 * j + 1] = *sample;
                            }
                            i += 2 * n;
                            if n < v.len() {
                                iter = Some(v.split_off(n));
                                return;
                            } else if i == data.len() {
                                return;
                            }
                        } else {
                            let n = std::cmp::min(v.len(), data.len() - i);
                            data[i..i + n].copy_from_slice(&v[..n]);
                            i += n;
                            if n < v.len() {
                                iter = Some(v.split_off(n));
                                return;
                            } else if i == data.len() {
                                return;
                            }
                        }
                    }
                },
                move |err| {
                    eprintln!("SDR FM audio stream error: {err:?}");
                },
                None,
            )
            .map_err(|e| {
                Error::RuntimeError(format!(
                    "Failed to open {} at {} Hz: {e}",
                    self.setup.device_label, self.setup.sample_rate
                ))
            })?;

        stream
            .play()
            .map_err(|e| Error::RuntimeError(format!("Failed to start audio playback: {e}")))?;

        self.tx = Some(tx);
        self.stream = Some(stream);
        Ok(())
    }

    async fn deinit(&mut self, _mo: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        if let Some(tx) = self.tx.as_mut() {
            let _ = tx.send(Vec::new()).await;
        }
        if let Some(t) = self.terminated.take() {
            let _ = t.await;
        }
        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let i_len = i.len();

        self.vec.extend_from_slice(i);
        if self.vec.len() >= self.min_buffer_size || self.input.finished() {
            self.tx
                .as_mut()
                .ok_or(Error::RuntimeError("audio sink not initialized".into()))?
                .send(std::mem::take(&mut self.vec))
                .await?;
        }

        self.input.consume(i_len);

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
