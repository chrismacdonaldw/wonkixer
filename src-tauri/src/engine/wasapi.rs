use std::slice;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_EVENT, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioClient3, IAudioClock,
    IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE,
};
use windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE;
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::System::Threading::{
    AvSetMmThreadCharacteristicsW, CreateEventW, SetEvent, WaitForMultipleObjects,
};
use windows::core::{HSTRING, w};

const WAKE_TIMEOUT_MS: u32 = 2_000;
const NANOS_PER_SECOND: u128 = 1_000_000_000;
const NANOS_PER_TICK: u128 = 100;
const THREAD_GONE: &str = "The output thread stopped unexpectedly.";
const STOP_SIGNALLED: WAIT_EVENT = WAIT_EVENT(WAIT_OBJECT_0.0 + 1);

pub type Callback = Box<dyn FnMut(&mut [f32], u128) + Send>;
pub type ErrorSink = Box<dyn FnMut(cpal::Error) + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub channels: u16,
    pub sample_rate: u32,
    pub period: usize,
}

pub fn promote_current_thread() {
    let mut task = 0;
    unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task) }.ok();
}

pub fn prepare(endpoint_id: &str) -> Result<Prepared, String> {
    let stop = Arc::new(Event::new()?);
    let (ready, format) = mpsc::sync_channel(1);
    let (begin, begun) = mpsc::sync_channel(1);
    let (report, started) = mpsc::sync_channel(1);
    let endpoint = endpoint_id.to_owned();
    let signal = Arc::clone(&stop);
    let thread = thread::Builder::new()
        .name("audio-render".to_owned())
        .spawn(move || run(&endpoint, &signal, ready, begun, report))
        .map_err(|error| format!("Could not start the output thread: {error}"))?;
    let render = Render {
        stop,
        thread: Some(thread),
    };
    let format = format.recv().map_err(|_| THREAD_GONE.to_owned())??;
    Ok(Prepared {
        begin,
        started,
        format,
        render,
    })
}

struct Begin {
    callback: Callback,
    on_error: ErrorSink,
}

pub struct Prepared {
    begin: SyncSender<Begin>,
    started: Receiver<Result<(), String>>,
    format: Format,
    render: Render,
}

impl Prepared {
    pub fn format(&self) -> Format {
        self.format
    }

    pub fn start(self, callback: Callback, on_error: ErrorSink) -> Result<Render, String> {
        self.begin
            .send(Begin { callback, on_error })
            .map_err(|_| THREAD_GONE.to_owned())?;
        self.started.recv().map_err(|_| THREAD_GONE.to_owned())??;
        Ok(self.render)
    }
}

pub struct Render {
    stop: Arc<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Render {
    fn drop(&mut self) {
        self.stop.set();
        if let Some(thread) = self.thread.take() {
            thread.join().ok();
        }
    }
}

fn run(
    endpoint: &str,
    stop: &Event,
    ready: SyncSender<Result<Format, String>>,
    begun: Receiver<Begin>,
    report: SyncSender<Result<(), String>>,
) {
    let Ok(_apartment) = Apartment::enter() else {
        ready.send(Err(THREAD_GONE.to_owned())).ok();
        return;
    };
    let stream = match Stream::open(endpoint) {
        Ok(stream) => stream,
        Err(error) => {
            ready.send(Err(error)).ok();
            return;
        }
    };
    ready.send(Ok(stream.format)).ok();
    let Ok(Begin {
        mut callback,
        mut on_error,
    }) = begun.recv()
    else {
        return;
    };
    promote_current_thread();
    if let Err(error) = stream.start() {
        report
            .send(Err(format!("Could not start the output device: {error}")))
            .ok();
        return;
    }
    report.send(Ok(())).ok();
    if let Err(error) = stream.pump(stop, &mut callback, &mut on_error) {
        on_error(cpal::Error::from(error));
    }
    stream.stop();
}

struct Stream {
    client: IAudioClient3,
    render: IAudioRenderClient,
    clock: IAudioClock,
    wake: Event,
    format: Format,
    buffer: u32,
    ticks_per_second: u128,
}

impl Stream {
    fn open(endpoint: &str) -> Result<Self, String> {
        let unavailable = |error: windows::core::Error| {
            format!("The output device has no low-latency mode: {error}")
        };
        unsafe {
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(unavailable)?;
            let device = devices
                .GetDevice(&HSTRING::from(endpoint))
                .map_err(unavailable)?;
            let client: IAudioClient3 = device.Activate(CLSCTX_ALL, None).map_err(unavailable)?;
            let mix = MixFormat(client.GetMixFormat().map_err(unavailable)?);
            let (channels, sample_rate) = float_layout(mix.0)
                .ok_or_else(|| "The output device does not mix in 32-bit float.".to_owned())?;
            let period = mix.smallest_period(&client).map_err(unavailable)?;
            client
                .InitializeSharedAudioStream(AUDCLNT_STREAMFLAGS_EVENTCALLBACK, period, mix.0, None)
                .map_err(unavailable)?;
            let wake = Event::new()?;
            client.SetEventHandle(wake.0).map_err(unavailable)?;
            let buffer = client.GetBufferSize().map_err(unavailable)?;
            let render = client.GetService().map_err(unavailable)?;
            let clock: IAudioClock = client.GetService().map_err(unavailable)?;
            let ticks_per_second = u128::from(clock.GetFrequency().map_err(unavailable)?).max(1);
            Ok(Self {
                client,
                render,
                clock,
                wake,
                format: Format {
                    channels,
                    sample_rate,
                    period: period as usize,
                },
                buffer,
                ticks_per_second,
            })
        }
    }

    fn start(&self) -> windows::core::Result<()> {
        unsafe {
            self.render.GetBuffer(self.buffer)?;
            self.render
                .ReleaseBuffer(self.buffer, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;
            self.client.Start()
        }
    }

    fn stop(&self) {
        unsafe { self.client.Stop() }.ok();
    }

    fn pump(
        &self,
        stop: &Event,
        callback: &mut Callback,
        on_error: &mut ErrorSink,
    ) -> windows::core::Result<()> {
        let handles = [self.wake.0, stop.0];
        let channels = usize::from(self.format.channels);
        let mut written = u64::from(self.buffer);
        loop {
            let woke = unsafe { WaitForMultipleObjects(&handles, false, WAKE_TIMEOUT_MS) };
            if woke == STOP_SIGNALLED {
                return Ok(());
            }
            if woke != WAIT_OBJECT_0 && woke != WAIT_TIMEOUT {
                return Err(windows::core::Error::from_thread());
            }
            let padding = unsafe { self.client.GetCurrentPadding()? };
            if woke == WAIT_TIMEOUT {
                continue;
            }
            if padding == 0 {
                on_error(cpal::Error::new(cpal::ErrorKind::Xrun));
            }
            let frames = self.buffer.saturating_sub(padding);
            if frames == 0 {
                continue;
            }
            let playback = self.playback(written)?;
            unsafe {
                let data = self.render.GetBuffer(frames)?;
                let samples =
                    slice::from_raw_parts_mut(data.cast::<f32>(), frames as usize * channels);
                callback(samples, playback);
                self.render.ReleaseBuffer(frames, 0)?;
            }
            written += u64::from(frames);
        }
    }

    fn playback(&self, written: u64) -> windows::core::Result<u128> {
        let (mut position, mut instant) = (0u64, 0u64);
        unsafe { self.clock.GetPosition(&mut position, Some(&mut instant))? };
        let played = u128::from(position) * NANOS_PER_SECOND / self.ticks_per_second;
        let queued = u128::from(written) * NANOS_PER_SECOND / u128::from(self.format.sample_rate);
        Ok(u128::from(instant) * NANOS_PER_TICK + queued.saturating_sub(played))
    }
}

unsafe fn float_layout(format: *const WAVEFORMATEX) -> Option<(u16, u32)> {
    let header = unsafe { format.read_unaligned() };
    let float = match u32::from(header.wFormatTag) {
        WAVE_FORMAT_IEEE_FLOAT => true,
        WAVE_FORMAT_EXTENSIBLE => {
            let extensible = unsafe { format.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned() };
            let sub_format = extensible.SubFormat;
            sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
        }
        _ => false,
    };
    (float && header.wBitsPerSample == 32 && header.nChannels > 0)
        .then_some((header.nChannels, header.nSamplesPerSec))
}

struct MixFormat(*mut WAVEFORMATEX);

impl MixFormat {
    fn smallest_period(&self, client: &IAudioClient3) -> windows::core::Result<u32> {
        let (mut default, mut fundamental, mut smallest, mut largest) = (0, 0, 0, 0);
        unsafe {
            client.GetSharedModeEnginePeriod(
                self.0,
                &mut default,
                &mut fundamental,
                &mut smallest,
                &mut largest,
            )?;
        }
        Ok(smallest.max(1))
    }
}

impl Drop for MixFormat {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.cast_const().cast())) };
    }
}

struct Event(HANDLE);

unsafe impl Send for Event {}
unsafe impl Sync for Event {}

impl Event {
    fn new() -> Result<Self, String> {
        unsafe { CreateEventW(None, false, false, None) }
            .map(Self)
            .map_err(|error| format!("Could not create an audio event: {error}"))
    }

    fn set(&self) {
        unsafe { SetEvent(self.0) }.ok();
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) }.ok();
    }
}

struct Apartment;

impl Apartment {
    fn enter() -> windows::core::Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?;
        Ok(Self)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::Media::Audio::{WAVE_FORMAT_PCM, WAVEFORMATEXTENSIBLE_0};
    use windows::Win32::Media::KernelStreaming::KSDATAFORMAT_SUBTYPE_PCM;

    use super::*;

    fn header(tag: u32, channels: u16, bits: u16) -> WAVEFORMATEX {
        WAVEFORMATEX {
            wFormatTag: tag as u16,
            nChannels: channels,
            nSamplesPerSec: 48_000,
            nAvgBytesPerSec: 48_000 * u32::from(channels * bits / 8),
            nBlockAlign: channels * bits / 8,
            wBitsPerSample: bits,
            cbSize: 0,
        }
    }

    fn extensible(sub_format: windows::core::GUID) -> WAVEFORMATEXTENSIBLE {
        WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                cbSize: 22,
                ..header(WAVE_FORMAT_EXTENSIBLE, 2, 32)
            },
            Samples: WAVEFORMATEXTENSIBLE_0 {
                wValidBitsPerSample: 32,
            },
            dwChannelMask: 3,
            SubFormat: sub_format,
        }
    }

    #[test]
    fn accepts_a_32_bit_float_mix() {
        let plain = header(WAVE_FORMAT_IEEE_FLOAT, 1, 32);
        let wrapped = extensible(KSDATAFORMAT_SUBTYPE_IEEE_FLOAT);

        assert_eq!(unsafe { float_layout(&plain) }, Some((1, 48_000)));
        assert_eq!(
            unsafe { float_layout((&raw const wrapped).cast()) },
            Some((2, 48_000))
        );
    }

    #[test]
    fn leaves_integer_mixes_to_the_shared_path() {
        let plain = header(WAVE_FORMAT_PCM, 2, 16);
        let wrapped = extensible(KSDATAFORMAT_SUBTYPE_PCM);

        assert_eq!(unsafe { float_layout(&plain) }, None);
        assert_eq!(unsafe { float_layout((&raw const wrapped).cast()) }, None);
    }

    #[test]
    fn an_unknown_endpoint_fails_without_leaving_a_thread_behind() {
        let started = std::time::Instant::now();

        assert!(prepare("{0.0.0.00000000}.{00000000-0000-0000-0000-000000000000}").is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
