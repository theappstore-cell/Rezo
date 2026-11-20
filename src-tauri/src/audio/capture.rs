use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle, Thread};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rtrb::Producer;

#[derive(Clone, Debug, PartialEq)]
pub enum CaptureTarget {
    System,
    Process { pid: u32 },
}

#[derive(Default)]
pub struct CaptureStatus {
    pub running: AtomicBool,
    pub error: Mutex<Option<String>>,
}

pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    pub status: Arc<CaptureStatus>,
}

impl CaptureHandle {
    pub fn stop(mut self) {
        self.shutdown();
    }
    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn start(target: CaptureTarget, producer: Producer<f32>, wake: Thread, pin_engine_period: bool) -> CaptureHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let status = Arc::new(CaptureStatus::default());
    let (s2, st2) = (stop.clone(), status.clone());
    let join = thread::Builder::new()
        .name("rezo-capture".into())
        .spawn(move || imp::thread_main(target, producer, wake, pin_engine_period, s2, st2))
        .expect("démarrage du thread de capture");
    CaptureHandle { stop, join: Some(join), status }
}

struct Pusher {
    prod: Producer<f32>,
    pushed: u64,
    wake: Thread,
    tmp: Vec<f32>,
    step: f64,
    pos: f64,
    last: [f32; 2],
}

impl Pusher {
    fn new(prod: Producer<f32>, wake: Thread) -> Self {
        Self { prod, pushed: 0, wake, tmp: Vec::with_capacity(16384), step: 1.0, pos: 0.0, last: [0.0; 2] }
    }

    fn set_rate(&mut self, rate: u32) {
        self.step = rate as f64 / super::SAMPLE_RATE as f64;
        self.pos = 0.0;
    }

    fn push_stereo(&mut self, input: &[f32]) {
        if (self.step - 1.0).abs() < 1e-9 {
            self.flush(input);
            return;
        }
        self.tmp.clear();
        for f in input.chunks_exact(2) {
            while self.pos < 1.0 {
                let t = self.pos as f32;
                self.tmp.push(self.last[0] + (f[0] - self.last[0]) * t);
                self.tmp.push(self.last[1] + (f[1] - self.last[1]) * t);
                self.pos += self.step;
            }
            self.pos -= 1.0;
            self.last = [f[0], f[1]];
        }
        let out = std::mem::take(&mut self.tmp);
        self.flush(&out);
        self.tmp = out;
    }

    fn push_silence(&mut self, frames: usize) {
        let mut left = frames * 2;
        let zeros = [0.0f32; 960];
        while left > 0 {
            let n = left.min(zeros.len());
            self.flush(&zeros[..n]);
            left -= n;
        }
    }

    fn flush(&mut self, stereo: &[f32]) {
        if stereo.is_empty() {
            return;
        }
        self.pushed += (stereo.len() / 2) as u64;
        if self.prod.slots() >= stereo.len() {
            if let Ok(chunk) = self.prod.write_chunk_uninit(stereo.len()) {
                chunk.fill_from_iter(stereo.iter().copied());
            }
        }
        self.wake.unpark();
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use std::sync::atomic::AtomicU32;

    use windows::core::{w, Interface, GUID, HRESULT, IUnknown, PROPVARIANT};
    use windows::Win32::Foundation::{CloseHandle, E_NOINTERFACE, E_POINTER, HANDLE, S_OK, WAIT_OBJECT_0};
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, IAgileObject, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};

    fn our_format() -> WAVEFORMATEX {
        WAVEFORMATEX {
            wFormatTag: 3,
            nChannels: 2,
            nSamplesPerSec: 48_000,
            nAvgBytesPerSec: 48_000 * 8,
            nBlockAlign: 8,
            wBitsPerSample: 32,
            cbSize: 0,
        }
    }

    #[derive(Clone, Copy)]
    struct Fmt {
        channels: usize,
        rate: u32,
        float: bool,
        bits: u16,
    }

    impl Fmt {
        fn from_wfx(w: &WAVEFORMATEX) -> Self {
            let tag = w.wFormatTag;
            let bits = w.wBitsPerSample;
            Fmt {
                channels: w.nChannels.max(1) as usize,
                rate: w.nSamplesPerSec,
                float: tag == 3 || (tag == 0xFFFE && bits == 32),
                bits,
            }
        }
    }

    pub fn thread_main(
        target: CaptureTarget,
        producer: Producer<f32>,
        wake: Thread,
        pin: bool,
        stop: Arc<AtomicBool>,
        status: Arc<CaptureStatus>,
    ) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let _mmcss = super::super::promote_current_thread();

        let pinner = if pin && target == CaptureTarget::System && !unsafe { default_is_bluetooth() } {
            let s = stop.clone();
            thread::Builder::new().name("rezo-pin".into()).spawn(move || pin_engine_period(s)).ok()
        } else {
            None
        };

        let mut pusher = Pusher::new(producer, wake);
        while !stop.load(Ordering::Acquire) {
            match unsafe { run_session(&target, &mut pusher, &stop, &status) } {
                Ok(()) => {}
                Err(e) => {
                    log::warn!("capture : {e} — nouvelle tentative");
                    *status.error.lock() = Some(e.to_string());
                    status.running.store(false, Ordering::Release);
                    let t0 = Instant::now();
                    while t0.elapsed() < Duration::from_millis(500) && !stop.load(Ordering::Acquire) {
                        pusher.push_silence(240);
                        thread::sleep(Duration::from_millis(5));
                    }
                }
            }
        }
        status.running.store(false, Ordering::Release);
        if let Some(p) = pinner {
            let _ = p.join();
        }
    }

    fn device_id(dev: &IMMDevice) -> String {
        unsafe {
            match dev.GetId() {
                Ok(p) => {
                    let s = p.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(p.0 as *const _));
                    s
                }
                Err(_) => String::new(),
            }
        }
    }

    unsafe fn default_render() -> windows::core::Result<IMMDevice> {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        en.GetDefaultAudioEndpoint(eRender, eConsole)
    }

    unsafe fn default_is_bluetooth() -> bool {
        use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
        use windows::Win32::System::Com::STGM_READ;
        use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;
        const KEY_ENUMERATOR: PROPERTYKEY =
            PROPERTYKEY { fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0), pid: 24 };
        let Ok(dev) = default_render() else { return false };
        let Ok(store) = dev.OpenPropertyStore(STGM_READ) else { return false };
        let Ok(pv) = store.GetValue(&KEY_ENUMERATOR) else { return false };
        let Ok(p) = PropVariantToStringAlloc(&pv) else { return false };
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        s.to_uppercase().starts_with("BTH")
    }

    unsafe fn run_session(
        target: &CaptureTarget,
        pusher: &mut Pusher,
        stop: &AtomicBool,
        status: &CaptureStatus,
    ) -> anyhow::Result<()> {
        let (client, fmt, watch_id) = match target {
            CaptureTarget::System => {
                let dev = default_render()?;
                let id = device_id(&dev);
                let ours = our_format();
                let flags = AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
                let c: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
                if c.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 0, 0, &ours, None).is_ok() {
                    (c, Fmt::from_wfx(&ours), Some(id))
                } else {
                    let c: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
                    let mix = c.GetMixFormat()?;
                    let fmt = Fmt::from_wfx(&*mix);
                    let flags = AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
                    let r = c.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 0, 0, mix, None);
                    CoTaskMemFree(Some(mix as *const _));
                    r?;
                    (c, fmt, Some(id))
                }
            }
            CaptureTarget::Process { pid } => {
                let c = activate_process_loopback(*pid)?;
                let ours = our_format();
                let flags = AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
                c.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 200_000, 0, &ours, None)?;
                (c, Fmt::from_wfx(&ours), None)
            }
        };

        pusher.set_rate(fmt.rate);
        let event = CreateEventW(None, false, false, None)?;
        client.SetEventHandle(event)?;
        let cap: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        status.running.store(true, Ordering::Release);
        *status.error.lock() = None;
        log::info!("capture démarrée ({:?}, {} Hz, {} ch)", target, fmt.rate, fmt.channels);

        let mut last_data = Instant::now();
        let mut silence_clock = Instant::now();
        let mut idle = false;
        let session_start = Instant::now();
        let pushed_at_start = pusher.pushed;
        let mut last_silent = true;
        let mut last_dev_check = Instant::now();
        let mut result: anyhow::Result<()> = Ok(());

        'outer: while !stop.load(Ordering::Acquire) {
            let w = WaitForSingleObject(event, if idle { 20 } else { 10 });
            let mut got = false;

            loop {
                let n = match cap.GetNextPacketSize() {
                    Ok(n) => n,
                    Err(e) => {
                        result = Err(e.into());
                        break 'outer;
                    }
                };
                if n == 0 {
                    break;
                }
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut frames = 0u32;
                let mut flags = 0u32;
                if let Err(e) = cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None) {
                    result = Err(e.into());
                    break 'outer;
                }
                let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
                last_silent = silent;
                convert_and_push(pusher, data, frames as usize, silent, &fmt);
                let _ = cap.ReleaseBuffer(frames);
                got = true;
            }

            let now = Instant::now();
            // A muted or idle endpoint can deliver fewer silent frames than real time (seen on S/PDIF outputs
            // muted by Smart Output): top up with silence so receivers never drain their buffers.
            if last_silent || !got {
                let due = now.duration_since(session_start).as_secs_f64() * 48_000.0;
                let behind = due - (pusher.pushed - pushed_at_start) as f64;
                if behind > 1440.0 {
                    pusher.push_silence((behind - 480.0) as usize);
                }
            }
            if got {
                last_data = now;
                idle = false;
            } else if w != WAIT_OBJECT_0 {
                if !idle && now.duration_since(last_data) > Duration::from_millis(25) {
                    idle = true;
                    silence_clock = now;
                }
                if idle {
                    let frames = (now.duration_since(silence_clock).as_secs_f64() * 48_000.0) as usize;
                    if frames > 0 {
                        pusher.push_silence(frames);
                        silence_clock += Duration::from_secs_f64(frames as f64 / 48_000.0);
                    }
                }
            }

            if let Some(id) = &watch_id {
                if now.duration_since(last_dev_check) > Duration::from_secs(1) {
                    last_dev_check = now;
                    if let Ok(d) = default_render() {
                        if &device_id(&d) != id {
                            log::info!("périphérique de sortie par défaut modifié : recapture");
                            break;
                        }
                    }
                }
            }
        }

        let _ = client.Stop();
        let _ = CloseHandle(event);
        result
    }

    unsafe fn convert_and_push(p: &mut Pusher, data: *mut u8, frames: usize, silent: bool, fmt: &Fmt) {
        if silent || data.is_null() {
            let out_frames = ((frames as f64) / p.step).round() as usize;
            p.push_silence(out_frames);
            return;
        }
        let ch = fmt.channels;
        let mut stereo: Vec<f32>;
        let buf: &[f32] = if fmt.float && ch == 2 {
            std::slice::from_raw_parts(data as *const f32, frames * 2)
        } else {
            stereo = Vec::with_capacity(frames * 2);
            for i in 0..frames {
                let get = |c: usize| -> f32 {
                    let c = c.min(ch - 1);
                    if fmt.float {
                        *(data as *const f32).add(i * ch + c)
                    } else if fmt.bits == 16 {
                        *(data as *const i16).add(i * ch + c) as f32 / 32768.0
                    } else if fmt.bits == 32 {
                        *(data as *const i32).add(i * ch + c) as f32 / 2147483648.0
                    } else {
                        0.0
                    }
                };
                stereo.push(get(0));
                stereo.push(get(1));
            }
            &stereo
        };
        p.push_stereo(buf);
    }

    // Hand-written COM callback: windows 0.58 does not generate the *_Impl trait for this interface.
    #[repr(C)]
    struct HandlerVtbl {
        query_interface: unsafe extern "system" fn(*mut Handler, *const GUID, *mut *mut c_void) -> HRESULT,
        add_ref: unsafe extern "system" fn(*mut Handler) -> u32,
        release: unsafe extern "system" fn(*mut Handler) -> u32,
        activate_completed: unsafe extern "system" fn(*mut Handler, *mut c_void) -> HRESULT,
    }

    #[repr(C)]
    struct Handler {
        vtbl: *const HandlerVtbl,
        refs: AtomicU32,
        event: HANDLE,
    }

    static HANDLER_VTBL: HandlerVtbl = HandlerVtbl {
        query_interface: h_query_interface,
        add_ref: h_add_ref,
        release: h_release,
        activate_completed: h_activate_completed,
    };

    unsafe extern "system" fn h_query_interface(this: *mut Handler, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
        if iid.is_null() || out.is_null() {
            return E_POINTER;
        }
        let iid = &*iid;
        if *iid == IUnknown::IID || *iid == IActivateAudioInterfaceCompletionHandler::IID || *iid == IAgileObject::IID {
            *out = this as *mut c_void;
            h_add_ref(this);
            S_OK
        } else {
            *out = std::ptr::null_mut();
            E_NOINTERFACE
        }
    }

    unsafe extern "system" fn h_add_ref(this: *mut Handler) -> u32 {
        (*this).refs.fetch_add(1, Ordering::AcqRel) + 1
    }

    unsafe extern "system" fn h_release(this: *mut Handler) -> u32 {
        let n = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
        if n == 0 {
            drop(Box::from_raw(this));
        }
        n
    }

    unsafe extern "system" fn h_activate_completed(this: *mut Handler, _op: *mut c_void) -> HRESULT {
        let _ = SetEvent((*this).event);
        S_OK
    }

    #[repr(C)]
    struct RawBlobPropVariant {
        vt: u16,
        reserved: [u16; 3],
        cb_size: u32,
        p_blob_data: *mut u8,
    }
    const VT_BLOB: u16 = 65;

    unsafe fn activate_process_loopback(pid: u32) -> anyhow::Result<IAudioClient> {
        let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: pid,
                    ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                },
            },
        };
        let pv = RawBlobPropVariant {
            vt: VT_BLOB,
            reserved: [0; 3],
            cb_size: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
            p_blob_data: &mut params as *mut _ as *mut u8,
        };

        let event = CreateEventW(None, false, false, None)?;
        let raw = Box::into_raw(Box::new(Handler { vtbl: &HANDLER_VTBL, refs: AtomicU32::new(1), event }));
        let handler = IActivateAudioInterfaceCompletionHandler::from_raw(raw as *mut c_void);
        let op = ActivateAudioInterfaceAsync(
            w!("VAD\\Process_Loopback"),
            &IAudioClient::IID,
            Some(&pv as *const RawBlobPropVariant as *const PROPVARIANT),
            &handler,
        )?;
        let waited = WaitForSingleObject(event, 5000);
        let _ = CloseHandle(event);
        if waited != WAIT_OBJECT_0 {
            anyhow::bail!("délai dépassé à l'activation du Process Loopback");
        }
        let mut hr = HRESULT(0);
        let mut unk: Option<windows::core::IUnknown> = None;
        op.GetActivateResult(&mut hr, &mut unk)?;
        hr.ok()?;
        let client = unk
            .ok_or_else(|| anyhow::anyhow!("interface audio absente"))?
            .cast::<IAudioClient>()?;
        Ok(client)
    }

    fn pin_engine_period(stop: Arc<AtomicBool>) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _mmcss = super::super::promote_current_thread();
            if let Err(e) = pin_inner(&stop) {
                log::info!("ancrage de la période audio indisponible ({e}) : horloge de secours seule");
            }
        }
    }

    unsafe fn pin_inner(stop: &AtomicBool) -> anyhow::Result<()> {
        let dev = default_render()?;
        let client: IAudioClient3 = dev.Activate(CLSCTX_ALL, None)?;
        let mix = client.GetMixFormat()?;
        let (mut def, mut fund, mut min, mut max) = (0u32, 0u32, 0u32, 0u32);
        client.GetSharedModeEnginePeriod(mix, &mut def, &mut fund, &mut min, &mut max)?;
        if min >= def {
            CoTaskMemFree(Some(mix as *const _));
            log::info!("période du moteur audio non réductible ({def} trames) : flux d'ancrage inutile, arrêté");
            return Ok(());
        }
        let r = client.InitializeSharedAudioStream(AUDCLNT_STREAMFLAGS_EVENTCALLBACK, min, mix, None);
        CoTaskMemFree(Some(mix as *const _));
        r?;
        log::info!("période du moteur audio ancrée à {min} trames (défaut {def})");

        let event = CreateEventW(None, false, false, None)?;
        client.SetEventHandle(event)?;
        let render: IAudioRenderClient = client.GetService()?;
        let total = client.GetBufferSize()?;
        client.Start()?;
        while !stop.load(Ordering::Acquire) {
            if WaitForSingleObject(event, 100) != WAIT_OBJECT_0 {
                continue;
            }
            let pad = client.GetCurrentPadding()?;
            let n = total.saturating_sub(pad);
            if n > 0 {
                render.GetBuffer(n)?;
                render.ReleaseBuffer(n, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)?;
            }
        }
        let _ = client.Stop();
        let _ = CloseHandle(event);
        Ok(())
    }
}

// macOS and Linux have no system loopback in cpal: Rezo records an input device, preferring virtual
// loopback drivers (BlackHole, Loopback, Soundflower, PulseAudio/PipeWire monitors).
#[cfg(not(windows))]
mod imp {
    use super::*;
    use anyhow::{anyhow, bail};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    const LOOPBACK_HINTS: [&str; 5] = ["blackhole", "loopback", "soundflower", "monitor", "rezo"];

    fn pick_device(host: &cpal::Host) -> Option<cpal::Device> {
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                let name = d.name().unwrap_or_default().to_lowercase();
                if LOOPBACK_HINTS.iter().any(|h| name.contains(h)) {
                    return Some(d);
                }
            }
        }
        host.default_input_device()
    }

    fn build<T: cpal::SizedSample + Send + 'static>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        pusher: Arc<Mutex<Pusher>>,
        failed: Arc<AtomicBool>,
        conv: fn(T) -> f32,
    ) -> Result<cpal::Stream, cpal::BuildStreamError> {
        let channels = config.channels.max(1) as usize;
        let mut stereo: Vec<f32> = Vec::with_capacity(16384);
        device.build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                stereo.clear();
                for frame in data.chunks_exact(channels) {
                    let l = conv(frame[0]);
                    let r = if channels > 1 { conv(frame[1]) } else { l };
                    stereo.push(l);
                    stereo.push(r);
                }
                pusher.lock().push_stereo(&stereo);
            },
            move |e| {
                log::warn!("capture : {e}");
                failed.store(true, Ordering::Release);
            },
            None,
        )
    }

    fn run_session(pusher: &Arc<Mutex<Pusher>>, stop: &AtomicBool, status: &CaptureStatus) -> anyhow::Result<()> {
        let host = cpal::default_host();
        let device = pick_device(&host).ok_or_else(|| anyhow!("aucun périphérique d'entrée audio"))?;
        let name = device.name().unwrap_or_default();
        let supported = device.default_input_config()?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        pusher.lock().set_rate(config.sample_rate.0);
        let failed = Arc::new(AtomicBool::new(false));
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, pusher.clone(), failed.clone(), |s| s)?,
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, pusher.clone(), failed.clone(), |s| s as f32 / 32768.0)?,
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, pusher.clone(), failed.clone(), |s| (s as f32 - 32768.0) / 32768.0)?,
            cpal::SampleFormat::I32 => build::<i32>(&device, &config, pusher.clone(), failed.clone(), |s| s as f32 / 2_147_483_648.0)?,
            other => bail!("format d'échantillon non géré : {other:?}"),
        };
        stream.play()?;
        status.running.store(true, Ordering::Release);
        *status.error.lock() = None;
        log::info!("capture démarrée ({name}, {} Hz, {} ch)", config.sample_rate.0, config.channels);
        while !stop.load(Ordering::Acquire) && !failed.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(50));
        }
        drop(stream);
        if failed.load(Ordering::Acquire) {
            bail!("flux d'entrée audio interrompu");
        }
        Ok(())
    }

    pub fn thread_main(
        target: CaptureTarget,
        producer: Producer<f32>,
        wake: Thread,
        _pin: bool,
        stop: Arc<AtomicBool>,
        status: Arc<CaptureStatus>,
    ) {
        let _mmcss = super::super::promote_current_thread();
        if let CaptureTarget::Process { .. } = target {
            log::warn!("capture d'une seule application indisponible sur ce système : capture du périphérique d'entrée");
        }
        let pusher = Arc::new(Mutex::new(Pusher::new(producer, wake)));
        while !stop.load(Ordering::Acquire) {
            if let Err(e) = run_session(&pusher, &stop, &status) {
                log::warn!("capture : {e} — nouvelle tentative");
                *status.error.lock() = Some(e.to_string());
                status.running.store(false, Ordering::Release);
                let t0 = Instant::now();
                while t0.elapsed() < Duration::from_secs(1) && !stop.load(Ordering::Acquire) {
                    pusher.lock().push_silence(240);
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }
        status.running.store(false, Ordering::Release);
    }
}
