use std::slice;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_EVENT, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioClient3, IAudioClock,
    IAudioRenderClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE,
};
use windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE;
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects};
use windows::core::HSTRING;

use super::priority;

const WAKE_TIMEOUT_MS: u32 = 2_000;
const NANOS_PER_SECOND: u128 = 1_000_000_000;
const NANOS_PER_TICK: u128 = 100;
const THREAD_GONE: &str = "The output thread stopped unexpectedly.";
const STALLED: &str = "it has not asked for audio for 2 seconds.";
const THREAD_ENDED: &str = "its audio thread ended unexpectedly.";
const NOT_FLOAT: &str = "The output device does not mix in 32-bit float.";
const STOP_SIGNALLED: WAIT_EVENT = WAIT_OBJECT_0;
const WAKE_SIGNALLED: WAIT_EVENT = WAIT_EVENT(WAIT_OBJECT_0.0 + 1);

pub type Callback = Box<dyn FnMut(&mut [f32], u128) + Send>;
pub type ErrorSink = Box<dyn FnMut(cpal::Error) + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub channels: u16,
    pub sample_rate: u32,
    pub period: usize,
    pub default_period: usize,
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
    let mut prepared = Prepared {
        begin: Some(begin),
        started,
        format: None,
        render: Some(Render {
            stop,
            thread: Some(thread),
        }),
    };
    prepared.format = Some(format.recv().map_err(|_| THREAD_GONE.to_owned())??);
    Ok(prepared)
}

struct Begin {
    callback: Callback,
    on_error: ErrorSink,
}

pub struct Prepared {
    begin: Option<SyncSender<Begin>>,
    started: Receiver<Result<(), String>>,
    format: Option<Format>,
    render: Option<Render>,
}

impl Prepared {
    pub fn format(&self) -> Format {
        self.format.expect("a prepared output knows its format")
    }

    pub fn start(mut self, callback: Callback, on_error: ErrorSink) -> Result<Render, String> {
        self.begin
            .take()
            .ok_or_else(|| THREAD_GONE.to_owned())?
            .send(Begin { callback, on_error })
            .map_err(|_| THREAD_GONE.to_owned())?;
        self.started.recv().map_err(|_| THREAD_GONE.to_owned())??;
        self.render.take().ok_or_else(|| THREAD_GONE.to_owned())
    }
}

impl Drop for Prepared {
    fn drop(&mut self) {
        drop(self.begin.take());
        drop(self.render.take());
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
    let _apartment = match Apartment::enter() {
        Ok(apartment) => apartment,
        Err(error) => {
            ready
                .send(Err(format!("Could not set up the output thread: {error}")))
                .ok();
            return;
        }
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
    priority::promote_current_thread();
    if let Err(error) = stream.start() {
        report
            .send(Err(format!("Could not start the output device: {error}")))
            .ok();
        return;
    }
    report.send(Ok(())).ok();
    let mut watch = Watch::new(&mut on_error);
    match stream.pump(stop, &mut callback, &mut watch) {
        Ok(()) => watch.stopped(),
        Err(error) => watch.failed(error),
    }
    stream.stop();
}

struct Watch<'sink> {
    on_error: &'sink mut ErrorSink,
    settled: bool,
}

impl<'sink> Watch<'sink> {
    fn new(on_error: &'sink mut ErrorSink) -> Self {
        Self {
            on_error,
            settled: false,
        }
    }

    fn glitch(&mut self) {
        (self.on_error)(cpal::Error::new(cpal::ErrorKind::Xrun));
    }

    fn failed(mut self, error: cpal::Error) {
        self.settled = true;
        (self.on_error)(error);
    }

    fn stopped(mut self) {
        self.settled = true;
    }
}

impl Drop for Watch<'_> {
    fn drop(&mut self) {
        if !self.settled {
            (self.on_error)(cpal::Error::with_message(
                cpal::ErrorKind::BackendError,
                THREAD_ENDED,
            ));
        }
    }
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
        let refused = |error: windows::core::Error| {
            format!("The output device has no low-latency mode: {error}")
        };
        unsafe {
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(refused)?;
            let device = devices
                .GetDevice(&HSTRING::from(endpoint))
                .map_err(refused)?;
            let (client, mix) = activate(&device).map_err(refused)?;
            let (channels, sample_rate) =
                float_layout(mix.0).ok_or_else(|| NOT_FLOAT.to_owned())?;
            let (default_period, smallest) = engine_periods(&client, &mix).map_err(refused)?;
            if smallest >= default_period {
                return Err(format!(
                    "The output device's shortest shared-mode period is its default, {default_period} frames."
                ));
            }
            let client = match client.InitializeSharedAudioStream(
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                smallest,
                mix.0,
                None,
            ) {
                Ok(()) => client,
                Err(_) => retry_at_current_period(&device, default_period)?,
            };
            let period = current_period(&client).map_err(refused)?;
            if period >= default_period {
                return Err(format!(
                    "The output device runs at its default period, {period} frames."
                ));
            }
            let wake = Event::new()?;
            client.SetEventHandle(wake.0).map_err(refused)?;
            let buffer = client.GetBufferSize().map_err(refused)?;
            let render = client.GetService().map_err(refused)?;
            let clock: IAudioClock = client.GetService().map_err(refused)?;
            let ticks_per_second = u128::from(clock.GetFrequency().map_err(refused)?).max(1);
            Ok(Self {
                client,
                render,
                clock,
                wake,
                format: Format {
                    channels,
                    sample_rate,
                    period: period as usize,
                    default_period: default_period as usize,
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
        watch: &mut Watch,
    ) -> Result<(), cpal::Error> {
        let handles = [stop.0, self.wake.0];
        let channels = usize::from(self.format.channels);
        let mut written = u64::from(self.buffer);
        loop {
            let woke = unsafe { WaitForMultipleObjects(&handles, false, WAKE_TIMEOUT_MS) };
            if woke == STOP_SIGNALLED {
                return Ok(());
            }
            if woke != WAKE_SIGNALLED && woke != WAIT_TIMEOUT {
                return Err(windows::core::Error::from_thread().into());
            }
            let padding = unsafe { self.client.GetCurrentPadding()? };
            if woke == WAIT_TIMEOUT {
                return Err(cpal::Error::with_message(
                    cpal::ErrorKind::BackendError,
                    STALLED,
                ));
            }
            let Refill { frames, starved } = refill(self.buffer, padding);
            if starved {
                watch.glitch();
            }
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
        Ok(playback_instant(
            Position {
                ticks: position,
                ticks_per_second: self.ticks_per_second,
                instant,
            },
            written,
            self.format.sample_rate,
        ))
    }
}

unsafe fn activate(device: &IMMDevice) -> windows::core::Result<(IAudioClient3, MixFormat)> {
    unsafe {
        let client: IAudioClient3 = device.Activate(CLSCTX_ALL, None)?;
        let mix = MixFormat(client.GetMixFormat()?);
        Ok((client, mix))
    }
}

unsafe fn engine_periods(
    client: &IAudioClient3,
    mix: &MixFormat,
) -> windows::core::Result<(u32, u32)> {
    let (mut default, mut fundamental, mut smallest, mut largest) = (0, 0, 0, 0);
    unsafe {
        client.GetSharedModeEnginePeriod(
            mix.0,
            &mut default,
            &mut fundamental,
            &mut smallest,
            &mut largest,
        )?;
    }
    Ok((default, smallest.max(1)))
}

unsafe fn current_period(client: &IAudioClient3) -> windows::core::Result<u32> {
    let mut format = std::ptr::null_mut();
    let mut period = 0;
    unsafe { client.GetCurrentSharedModeEnginePeriod(&mut format, &mut period)? };
    drop(MixFormat(format));
    Ok(period)
}

unsafe fn retry_at_current_period(
    device: &IMMDevice,
    default_period: u32,
) -> Result<IAudioClient3, String> {
    let refused = |error: windows::core::Error| {
        format!("The output device refused a low-latency period: {error}")
    };
    unsafe {
        let (client, mix) = activate(device).map_err(refused)?;
        let current = current_period(&client).map_err(refused)?;
        if current == 0 || current >= default_period {
            return Err(format!(
                "Another app holds the output device at {current} frames, its default period."
            ));
        }
        client
            .InitializeSharedAudioStream(AUDCLNT_STREAMFLAGS_EVENTCALLBACK, current, mix.0, None)
            .map_err(refused)?;
        Ok(client)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Refill {
    frames: u32,
    starved: bool,
}

fn refill(buffer: u32, padding: u32) -> Refill {
    Refill {
        frames: buffer.saturating_sub(padding),
        starved: padding == 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Position {
    ticks: u64,
    ticks_per_second: u128,
    instant: u64,
}

fn playback_instant(position: Position, written: u64, sample_rate: u32) -> u128 {
    let played = u128::from(position.ticks) * NANOS_PER_SECOND / position.ticks_per_second;
    let queued = u128::from(written) * NANOS_PER_SECOND / u128::from(sample_rate.max(1));
    u128::from(position.instant) * NANOS_PER_TICK + queued.saturating_sub(played)
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

impl Drop for MixFormat {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CoTaskMemFree(Some(self.0.cast_const().cast())) };
        }
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
    use std::sync::Mutex;

    use windows::Win32::Media::Audio::{WAVE_FORMAT_PCM, WAVEFORMATEXTENSIBLE_0};
    use windows::Win32::Media::KernelStreaming::KSDATAFORMAT_SUBTYPE_PCM;

    use super::*;

    const SECOND: u128 = 1_000_000_000;

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

    fn error_log() -> (Arc<Mutex<Vec<cpal::ErrorKind>>>, ErrorSink) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&log);
        (
            log,
            Box::new(move |error: cpal::Error| sink.lock().expect("log").push(error.kind())),
        )
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
    fn refills_what_the_device_consumed_and_flags_an_empty_buffer() {
        assert_eq!(
            refill(280, 152),
            Refill {
                frames: 128,
                starved: false
            }
        );
        assert_eq!(
            refill(280, 0),
            Refill {
                frames: 280,
                starved: true
            }
        );
        assert_eq!(
            refill(280, 300),
            Refill {
                frames: 0,
                starved: false
            }
        );
    }

    #[test]
    fn a_block_plays_once_everything_queued_ahead_of_it_has_played() {
        let position = Position {
            ticks: 384_000,
            ticks_per_second: 384_000,
            instant: 50_000_000,
        };

        assert_eq!(
            playback_instant(position, 48_000 + 480, 48_000),
            5 * SECOND + 10_000_000
        );
        assert_eq!(playback_instant(position, 24_000, 48_000), 5 * SECOND);
    }

    #[test]
    fn a_failed_pump_reports_its_error_once() {
        let (log, mut sink) = error_log();
        Watch::new(&mut sink).failed(cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable));

        assert_eq!(
            *log.lock().expect("log"),
            [cpal::ErrorKind::DeviceNotAvailable]
        );
    }

    #[test]
    fn a_pump_ended_by_the_stop_event_reports_nothing() {
        let (log, mut sink) = error_log();
        let mut watch = Watch::new(&mut sink);
        watch.glitch();
        watch.stopped();

        assert_eq!(*log.lock().expect("log"), [cpal::ErrorKind::Xrun]);
    }

    #[test]
    fn a_pump_that_unwinds_reports_a_failure() {
        let (log, mut sink) = error_log();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _watch = Watch::new(&mut sink);
            panic!("callback failed");
        }));

        assert!(unwound.is_err());
        assert_eq!(*log.lock().expect("log"), [cpal::ErrorKind::BackendError]);
    }

    #[test]
    fn an_unknown_endpoint_fails_without_leaving_a_thread_behind() {
        let started = std::time::Instant::now();

        assert!(prepare("{0.0.0.00000000}.{00000000-0000-0000-0000-000000000000}").is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
