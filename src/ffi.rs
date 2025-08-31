use std::{ffi::c_int, sync::LazyLock, time::Duration};

use crate::{
    Combinator, CombinatorType, Envelope, Operator, OperatorModifiers, Pom, Sample, SampleBank,
    SampleID, Waveform, time::NANOS_PER_SEC,
};

/// The `Pom` type used in FFI. Only one type of data is supported currently, and that is [`SampleBank`].
type FFIPomBox = Box<dyn Pom<SampleBank> + 'static>;

/// The pointer type for synthesisers sent through FFI (`Pom*` in C).
/// `Pom` should be an opaque type on the other end.
/// It is not a mistake that these are pointers to boxes; these are `leak`-ed `Box<FFIPomBox>`es.
type PomOpaqueMut = *mut FFIPomBox;
/// The pointer type for synthesisers sent through FFI (`const Pom*` in C).
/// `Pom` should be an opaque type on the other end.
/// It is not a mistake that these are pointers to boxes; these are `leak`-ed `Box<FFIPomBox>`es.
type PomOpaque = *const FFIPomBox;

/// The pointer type for sample banks sent through FFI (`PomSampleBank*` in C).
/// `PomSampleBank` should be an opaque type on the other end.
type PomPCMBankMut = *mut SampleBank;
/// The pointer type for sample banks sent through FFI (`const PomSampleBank*` in C).
/// `PomSampleBank` should be an opaque type on the other end.
type PomPCMBank = *const SampleBank;
static EMPTY_PCM_BANK: LazyLock<SampleBank> = LazyLock::new(|| SampleBank::default());

/// A duration type that can be transferred over FFI.
#[repr(C)]
pub struct PomDuration {
    seconds: u64,
    nanoseconds: u32,
}
impl PomDuration {
    pub fn to_rust(&self) -> Duration {
        if self
            .seconds
            .checked_add(self.nanoseconds as u64 / NANOS_PER_SEC as u64)
            .is_none()
        {
            Duration::MAX
        } else {
            Duration::new(
                self.seconds + self.nanoseconds as u64 / NANOS_PER_SEC as u64,
                self.nanoseconds % NANOS_PER_SEC,
            )
        }
    }
}
impl From<Duration> for PomDuration {
    fn from(value: Duration) -> Self {
        Self {
            seconds: value.as_secs(),
            nanoseconds: value.subsec_nanos(),
        }
    }
}

/// Data for a [`PomWaveform`].
#[repr(C)]
pub union PomWaveformData {
    constant_offset: f64,
    duty_cycle: f64,
    sample_id: SampleID,
}

/// Waveform settings for an operator.
#[repr(C)]
pub struct PomWaveform {
    ty: c_int,
    data: PomWaveformData,
}
impl PomWaveform {
    pub fn to_rust(&self) -> Option<Waveform> {
        match self.ty {
            0 => Some(Waveform::Sine),
            1 => Some(Waveform::Pulse {
                duty_cycle: unsafe { self.data.duty_cycle },
            }),
            2 => Some(Waveform::Triangle),
            3 => Some(Waveform::Sawtooth),
            4 => Some(Waveform::InvertedSawtooth),
            5 => Some(Waveform::PCM(unsafe { self.data.sample_id })),
            6 => Some(Waveform::Constant(unsafe { self.data.constant_offset })),
            _ => None,
        }
    }
}

/// An envelope for an operator.
#[repr(C)]
pub struct PomEnvelope {
    attack_time: PomDuration,
    halving_rate: f64,
    release_time: PomDuration,
}
impl PomEnvelope {
    pub fn to_rust(&self) -> Envelope {
        Envelope {
            attack_time: self.attack_time.to_rust(),
            halving_rate: self.halving_rate,
            release_time: self.release_time.to_rust(),
        }
    }
}

/// Modifiers that are applied to an operator.
#[repr(C)]
pub struct PomModifiers {
    frequency_multiplier: f64,
    volume_multiplier: f64,
    constant_phase_offset: f64,
}
impl PomModifiers {
    pub fn to_rust(&self) -> OperatorModifiers {
        OperatorModifiers {
            frequency_multiplier: self.frequency_multiplier,
            volume_multiplier: self.volume_multiplier,
            constant_phase_offset: self.constant_phase_offset,
        }
    }
}

/// Settings for creating an operator.
#[repr(C)]
pub struct PomOperatorSettings {
    waveform: PomWaveform,
    envelope: PomEnvelope,
    modifiers: PomModifiers,
}

/// Settings for creating a PCM sample.
#[repr(C)]
pub struct PomPCMSampleSettings {
    samples_per_period: f64,
    loop_point: PomDuration,
    loop_duration: PomDuration,
}

pub fn send_boxed_pom_to_ffi(
    output: &mut PomOpaqueMut,
    synth: FFIPomBox,
) -> PomResultCode {
    *output = Box::leak(Box::new(synth)) as PomOpaqueMut;
    PomResult::Success as PomResultCode
}
pub fn send_pom_to_ffi(
    output: &mut PomOpaqueMut,
    synth: impl Pom<SampleBank> + 'static,
) -> PomResultCode {
    send_boxed_pom_to_ffi(output, Box::new(synth))
}
/// SAFETY:
/// - `synth` must be an output of `send_to_ffi`.
/// - When the result is dropped, `synth` becomes a dangling pointer.
pub unsafe fn take_pom_from_ffi(synth: PomOpaqueMut) -> Box<FFIPomBox> {
    let synth = unsafe { Box::from_raw(synth) };
    synth
}
/// SAFETY: `synth` must be an output of `send_to_ffi`.
pub unsafe fn get_pom_from_ffi(synth: PomOpaque) -> &'static FFIPomBox {
    unsafe { synth.as_ref() }.unwrap()
}
/// SAFETY: `synth` must be an output of `send_to_ffi`.
pub unsafe fn get_mut_pom_from_ffi(synth: PomOpaqueMut) -> &'static mut FFIPomBox {
    unsafe { synth.as_mut() }.unwrap()
}
/// SAFETY: `synth` must be an output of `send_to_ffi`.
pub unsafe fn clone_pom_from_ffi(synth: PomOpaque) -> FFIPomBox {
    unsafe { get_pom_from_ffi(synth) }.box_clone()
}

pub fn send_pcm_bank_to_ffi(out: &mut PomPCMBankMut, bank: SampleBank) -> PomResultCode {
    *out = Box::leak(Box::new(bank));
    PomResult::Success as PomResultCode
}
pub fn create_ffi_pcm_bank(output: &mut PomPCMBankMut) -> PomResultCode {
    send_pcm_bank_to_ffi(output, SampleBank::default())
}
/// SAFETY:
/// - `bank` must be an output of `create_pcm_bank`.
/// - When the result is dropped, `bank` becomes a dangling pointer.
pub unsafe fn take_pcm_bank_from_ffi(bank: PomPCMBankMut) -> Box<SampleBank> {
    let bank = unsafe { Box::from_raw(bank) };
    bank
}
/// SAFETY: `bank` must be an output of `create_pcm_bank`, or null.
pub unsafe fn get_pcm_bank_from_ffi(bank: PomPCMBank) -> &'static SampleBank {
    unsafe { bank.as_ref() }.unwrap_or(&*EMPTY_PCM_BANK)
}
/// SAFETY: `bank` must be an output of `create_pcm_bank`.
pub unsafe fn get_mut_pcm_bank_from_ffi(bank: PomPCMBankMut) -> &'static mut SampleBank {
    unsafe { bank.as_mut() }.unwrap()
}
/// SAFETY: `bank` must be an output of `create_pcm_bank`, or null.
pub unsafe fn clone_pcm_bank_from_ffi(bank: PomPCMBank) -> SampleBank {
    unsafe { get_pcm_bank_from_ffi(bank) }.clone()
}

#[repr(i32)]
pub enum PomResult {
    Success = 0,
    InvalidInput = 1,
}
type PomResultCode = i32;

#[repr(i32)]
pub enum PomSampleFormat {
    U8,
    I16,
    I32,
    F32,
    F64,
}

#[unsafe(no_mangle)]
pub extern "C" fn pom_create_operator(
    output: &mut PomOpaqueMut,
    settings: PomOperatorSettings,
) -> PomResultCode {
    if let Some(waveform) = settings.waveform.to_rust() {
        send_pom_to_ffi(
            output,
            Operator::new(
                waveform,
                settings.envelope.to_rust(),
                settings.modifiers.to_rust(),
            ),
        )
    } else {
        PomResult::InvalidInput as PomResultCode
    }
}

/// SAFETY: `modulator` and `carrier` must be outputs of `send_to_ffi`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_create_modulator(
    output: &mut PomOpaqueMut,
    modulator: PomOpaque,
    carrier: PomOpaque,
) -> PomResultCode {
    send_pom_to_ffi(
        output,
        Combinator {
            synths: unsafe { vec![clone_pom_from_ffi(modulator), clone_pom_from_ffi(carrier)] },
            ty: CombinatorType::Modulate,
        },
    )
}

/// SAFETY: `a` and `b` must be outputs of `send_to_ffi`.
#[unsafe(no_mangle)]
pub extern "C" fn pom_create_summation(
    output: &mut PomOpaqueMut,
    a: PomOpaque,
    b: PomOpaque,
) -> PomResultCode {
    send_pom_to_ffi(
        output,
        Combinator {
            synths: unsafe { vec![clone_pom_from_ffi(a), clone_pom_from_ffi(b)] },
            ty: CombinatorType::Sum,
        },
    )
}

/// SAFETY: `synths` must be the base of a `length`-long array of outputs of `send_to_ffi`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_create_combinator(
    output: &mut PomOpaqueMut,
    synths: *const PomOpaque,
    length: u64,
    ty: c_int,
) -> PomResultCode {
    let ty = match ty {
        0 => CombinatorType::Sum,
        1 => CombinatorType::Modulate,
        _ => return PomResult::InvalidInput as PomResultCode,
    };
    let slice = unsafe { core::slice::from_raw_parts(synths, length as usize) };
    let synths = slice
        .iter()
        .copied()
        .map(|pom| unsafe { clone_pom_from_ffi(pom) })
        .collect::<Vec<_>>();
    send_pom_to_ffi(output, Combinator { synths, ty })
}

/// SAFETY: `synth` must be an output of `send_to_ffi`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_play(synth: PomOpaqueMut, frequency: f64, volume: f64) {
    unsafe { get_mut_pom_from_ffi(synth) }.play(frequency, volume);
}

/// SAFETY:
/// - `synth` must be an output of `send_to_ffi`.
/// - `bank` must be an output of `create_pcm_bank`, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_sample(
    synth: PomOpaqueMut,
    bank: PomPCMBank,
    global_time: PomDuration,
    input_phase_offset: f64,
) -> f64 {
    unsafe { get_mut_pom_from_ffi(synth) }
        .sample(
            unsafe { get_pcm_bank_from_ffi(bank) },
            global_time.to_rust(),
            input_phase_offset,
        )
        .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn pom_frequency_to_interval(frequency: f64) -> PomDuration {
    PomDuration::from(Duration::from_secs(1).div_f64(frequency))
}

/// Helper function for integer PCM.
/// Maps `x` from `input_min..input_max` to `output_min..output_max`, rounds, then clamps on the output range.
pub fn quantise(x: f64, input_min: f64, input_max: f64, output_min: f64, output_max: f64) -> f64 {
    ((x - input_min) / (input_max - input_min) * (output_max - output_min) + output_min)
        .round()
        .clamp(output_min, output_max)
}

fn get_sample_format(sample_format: c_int) -> Result<PomSampleFormat, PomResultCode> {
    Ok(match sample_format {
        0 => PomSampleFormat::U8,
        1 => PomSampleFormat::I16,
        2 => PomSampleFormat::I32,
        3 => PomSampleFormat::F32,
        4 => PomSampleFormat::F64,
        _ => return Err(PomResult::InvalidInput as PomResultCode),
    })
}

/// SAFETY:
/// - `synth` must be an output of `send_to_ffi`.
/// - `bank` must be an output of `create_pcm_bank`, or null.
/// - `data` must be the base of a `length`-long array of samples whose size is governed by `sample_format`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_fill(
    synth: PomOpaqueMut,
    bank: PomPCMBank,
    global_time: PomDuration,
    sample_interval: PomDuration,
    data: *mut (),
    length: u64,
    sample_format: c_int,
    constant_phase_offset: f64,
) -> PomResultCode {
    let length = length as usize;
    let synth = unsafe { get_mut_pom_from_ffi(synth) };
    let mut time = global_time.to_rust();
    let interval = sample_interval.to_rust();
    let sample_format = match get_sample_format(sample_format) {
        Ok(format) => format,
        Err(code) => return code,
    };
    let mut get = || -> f64 {
        let sample = synth
            .sample(
                unsafe { get_pcm_bank_from_ffi(bank) },
                time,
                constant_phase_offset,
            )
            .unwrap_or(0.0);
        time += interval;
        sample
    };
    match sample_format {
        PomSampleFormat::U8 => {
            let data: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(data.cast(), length) };
            for i in 0..length {
                data[i] = quantise(get(), -1.0, 1.0, u8::MIN as f64, u8::MAX as f64) as u8;
            }
        }
        PomSampleFormat::I16 => {
            let data: &mut [i16] = unsafe { core::slice::from_raw_parts_mut(data.cast(), length) };
            for i in 0..length {
                data[i] = quantise(get(), -1.0, 1.0, i16::MIN as f64, i16::MAX as f64) as i16;
            }
        }
        PomSampleFormat::I32 => {
            let data: &mut [i32] = unsafe { core::slice::from_raw_parts_mut(data.cast(), length) };
            for i in 0..length {
                data[i] = quantise(get(), -1.0, 1.0, i32::MIN as f64, i32::MAX as f64) as i32;
            }
        }
        PomSampleFormat::F32 => {
            let data: &mut [f32] = unsafe { core::slice::from_raw_parts_mut(data.cast(), length) };
            for i in 0..length {
                data[i] = get() as f32;
            }
        }
        PomSampleFormat::F64 => {
            let data: &mut [f64] = unsafe { core::slice::from_raw_parts_mut(data.cast(), length) };
            for i in 0..length {
                data[i] = get();
            }
        }
    }
    PomResult::Success as PomResultCode
}

/// SAFETY: `synth` must be an output of `send_to_ffi`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_release(synth: PomOpaqueMut) {
    unsafe { get_mut_pom_from_ffi(synth) }.release();
}

/// SAFETY: `synth` must be an output of `send_to_ffi`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_cut(synth: PomOpaqueMut) {
    unsafe { get_mut_pom_from_ffi(synth) }.cut();
}

/// SAFETY: `synth` must be an output of `send_to_ffi`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_destroy_synth(pom: PomOpaqueMut) {
    drop(unsafe { take_pom_from_ffi(pom) })
}

/// SAFETY: `synth` must be an output of `send_to_ffi`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_clone_synth(out: &mut PomOpaqueMut, source: PomOpaque) -> PomResultCode {
    send_boxed_pom_to_ffi(out, unsafe { clone_pom_from_ffi(source) })
}

fn map_normalise(x: f64, min: f64, max: f64) -> f64 {
    2.0 * (x - min) / (max - min) - 1.0
}

#[unsafe(no_mangle)]
pub extern "C" fn pom_create_pcm_bank(output: &mut PomPCMBankMut) -> PomResultCode {
    create_ffi_pcm_bank(output)
}

/// SAFETY:
/// - `bank` must be an output of `create_pcm_bank`.
/// - `data` must be the base of a `length`-long array of samples whose size is governed by `sample_format`, containing PCM data for the PCM sample.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_add_pcm_sample(
    bank: PomPCMBankMut,
    pcm_data: *const (),
    pcm_length: u64,
    pcm_sample_format: c_int,
    identifier: SampleID,
    pcm_sample_settings: PomPCMSampleSettings,
) -> PomResultCode {
    let pcm_length = pcm_length as usize;
    let sample_bank = unsafe { get_mut_pcm_bank_from_ffi(bank) };
    let sample_format = match get_sample_format(pcm_sample_format) {
        Ok(format) => format,
        Err(code) => return code,
    };
    let mut converted_data = vec![0.0; pcm_length];
    match sample_format {
        PomSampleFormat::U8 => {
            let data: &[u8] = unsafe { core::slice::from_raw_parts(pcm_data.cast(), pcm_length) };
            for i in 0..pcm_length {
                converted_data[i] = map_normalise(data[i] as f64, u8::MIN as f64, u8::MAX as f64);
            }
        }
        PomSampleFormat::I16 => {
            let data: &[i16] = unsafe { core::slice::from_raw_parts(pcm_data.cast(), pcm_length) };
            for i in 0..pcm_length {
                converted_data[i] = map_normalise(data[i] as f64, i16::MIN as f64, i16::MAX as f64);
            }
        }
        PomSampleFormat::I32 => {
            let data: &[i32] = unsafe { core::slice::from_raw_parts(pcm_data.cast(), pcm_length) };
            for i in 0..pcm_length {
                converted_data[i] = map_normalise(data[i] as f64, i32::MIN as f64, i32::MAX as f64);
            }
        }
        PomSampleFormat::F32 => {
            let data: &[f32] = unsafe { core::slice::from_raw_parts(pcm_data.cast(), pcm_length) };
            for i in 0..pcm_length {
                converted_data[i] = data[i] as f64;
            }
        }
        PomSampleFormat::F64 => {
            let data: &[f64] = unsafe { core::slice::from_raw_parts(pcm_data.cast(), pcm_length) };
            converted_data.copy_from_slice(data);
        }
    }
    sample_bank.samples.insert(
        identifier,
        Sample {
            samples_per_period: pcm_sample_settings.samples_per_period,
            loop_point: pcm_sample_settings.loop_point.to_rust(),
            loop_duration: pcm_sample_settings.loop_duration.to_rust(),
            pcm_data: converted_data,
        },
    );
    PomResult::Success as PomResultCode
}

/// SAFETY: `bank` must be an output of `create_ffi_pcm_bank`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_destroy_pcm_bank(bank: PomPCMBankMut) {
    drop(unsafe { take_pcm_bank_from_ffi(bank) })
}

/// SAFETY: `bank` must be an output of `create_ffi_pcm_bank`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pom_clone_pcm_bank(
    out: &mut PomPCMBankMut,
    bank: PomPCMBank,
) -> PomResultCode {
    send_pcm_bank_to_ffi(out, unsafe { clone_pcm_bank_from_ffi(bank) })
}
