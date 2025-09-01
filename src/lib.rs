#![feature(bigint_helper_methods)]

mod ffi;

use std::{collections::HashMap, f64::consts::TAU, time::Duration};

use decent::{Decodable, Encodable};
use decent_macros::Binary;

/// A looser definition of [`Duration`]. Every "second" is instead a period of a waveform.
/// Invaluable for fixed-point time math.
pub type Period = Duration;
pub type SampleID = u64;

/// Some time utilities used internally.
pub mod time {
    use std::time::Duration;

    pub const NANOS_PER_SEC: u32 = 1_000_000_000;
    pub fn wrap_duration(duration: Duration, max: Duration) -> Duration {
        let nanos = duration.as_nanos() % max.as_nanos();
        Duration::new(
            (nanos / NANOS_PER_SEC as u128) as u64,
            (nanos % NANOS_PER_SEC as u128) as u32,
        )
    }
    pub fn duration_saturating_mul_f64(lhs: Duration, rhs: f64) -> Duration {
        if rhs < 0.0 {
            return Duration::ZERO;
        }
        match Duration::try_from_secs_f64(rhs * lhs.as_secs_f64()) {
            Ok(duration) => duration,
            Err(_) => Duration::MAX,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, PartialOrd, Binary)]
pub struct Sample {
    pub samples_per_period: f64,
    pub loop_point: Period,
    pub loop_duration: Period,
    pub pcm_data: Vec<f64>,
}
impl Sample {
    /// Converts floating-point seconds into period locations.
    pub fn new(
        data: Vec<f64>,
        samples_per_second: f64,
        samples_per_period: f64,
        loop_point_secs: f64,
        loop_duration_secs: f64,
    ) -> Self {
        let periods_per_second = samples_per_second / samples_per_period;
        let loop_point_periods = Period::from_secs_f64(loop_point_secs * periods_per_second);
        let loop_duration_periods = Period::from_secs_f64(loop_duration_secs * periods_per_second);
        Self {
            samples_per_period,
            loop_point: loop_point_periods,
            loop_duration: loop_duration_periods,
            pcm_data: data,
        }
    }
    pub fn get(&self, mut period: Period, phase_offset: f64) -> f64 {
        if phase_offset < 0.0 {
            let negative_phase_offset_period = Period::from_secs_f64(-phase_offset);
            if negative_phase_offset_period > period {
                return 0.0;
            }
            period = period.saturating_sub(negative_phase_offset_period);
        } else {
            period = period.saturating_add(Period::from_secs_f64(phase_offset));
        }
        period = if period < self.loop_point {
            period
        } else {
            time::wrap_duration(period.saturating_sub(self.loop_point), self.loop_duration)
                .saturating_add(self.loop_point)
        };
        let sample_index =
            time::duration_saturating_mul_f64(period, self.samples_per_period).as_secs() as usize;
        self.pcm_data.get(sample_index).copied().unwrap_or(0.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Binary)]
pub struct SampleBank {
    pub samples: HashMap<SampleID, Sample>,
}

/// A waveform, with a phase wrapped to be within [0, 1).
#[derive(Clone, Debug, Default, PartialEq, PartialOrd, Binary)]
pub enum Waveform {
    #[default]
    /// A sinusoid.
    Sine,
    /// A pulse wave with a given duty cycle.
    Pulse { duty_cycle: f64 },
    /// A triangle wave.
    Triangle,
    /// A sawtooth wave.
    Sawtooth,
    /// A sawtooth wave, but negated.
    InvertedSawtooth,
    /// A PCM sample from a [`SampleBank`].
    PCM(SampleID),

    /// A constant, unchanging value.
    Constant(f64),
    /// Transforms the phase domain of `base` to be [0, `waveform_active_percent`), with phases outside of the domain returning 0.
    Thin {
        base: Box<Waveform>,
        waveform_active_percent: f64,
    },
    /// Any phase greater than `waveform_active_percent` will produce a value of 0.
    Cut {
        base: Box<Waveform>,
        waveform_active_percent: f64,
    },
    /// Computes the absolute value of the output of a waveform.
    Absolute(Box<Waveform>),
}
impl Waveform {
    /// `period` should preferably *not* be wrapped before being passed into this function;
    /// PCM samples will not work properly.
    pub fn sample(&self, samples: &SampleBank, mut period: Period, phase_offset: f64) -> f64 {
        let monotonic_period = period;
        period = Period::from_nanos(period.subsec_nanos() as u64);
        let phase = (period.as_secs_f64() + phase_offset.rem_euclid(1.0)).rem_euclid(1.0);
        match self {
            Waveform::Sine => (phase * TAU).sin(),
            Waveform::Pulse { duty_cycle } => {
                if phase > *duty_cycle {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Triangle => {
                if phase < 0.5 {
                    phase * 4.0 - 1.0
                } else {
                    3.0 - phase * 4.0
                }
            }
            Waveform::Sawtooth => phase * 2.0 - 1.0,
            Waveform::InvertedSawtooth => phase * -2.0 + 1.0,
            Waveform::PCM(sample_id) => {
                let Some(sample) = samples.samples.get(sample_id) else {
                    return 0.0;
                };
                sample.get(monotonic_period, phase_offset)
            }

            Waveform::Constant(value) => *value,
            Waveform::Thin {
                base,
                waveform_active_percent,
            } => {
                if phase > *waveform_active_percent {
                    0.0
                } else {
                    base.sample(
                        samples,
                        Period::from_secs_f64(phase / *waveform_active_percent),
                        phase_offset,
                    )
                }
            }
            Waveform::Cut {
                base,
                waveform_active_percent,
            } => {
                if phase > *waveform_active_percent {
                    0.0
                } else {
                    base.sample(samples, period, phase_offset)
                }
            }
            Waveform::Absolute(base) => base.sample(samples, period, phase_offset).abs(),
        }
    }
}

/// An envelope consisting of a peak volume, attack time, halving rate, and release time.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Binary)]
pub struct Envelope {
    /// Linear attack time; the time it takes to reach peak volume.
    pub attack_time: Duration,
    /// Exponential decay; the amount of times the output volume halves in one second.
    /// Takes effect after attack time.
    pub halving_rate: f64,
    /// Linear release time; the time it takes to reach zero volume.
    /// Multiplied by the rest of the envelope.
    pub release_time: Duration,
}
impl Envelope {
    /// If `None`, the envelope has finished.
    pub fn sample_volume(&self, note_time: Duration, stop_point: Option<Duration>) -> Option<f64> {
        let release_multiplier = if let Some(stop_point) = stop_point {
            if note_time > stop_point.saturating_add(self.release_time) {
                return None;
            }
            let release_progress = note_time.saturating_sub(stop_point);
            let release_fraction = release_progress.as_secs_f64() / self.release_time.as_secs_f64();
            1.0 - release_fraction
        } else {
            1.0
        };

        if note_time < self.attack_time {
            let attack_fraction = note_time.as_secs_f64() / self.attack_time.as_secs_f64();
            Some(attack_fraction * release_multiplier)
        } else {
            let time_from_decay_start = note_time.saturating_sub(self.attack_time);
            let decay_multiplier =
                0.5f64.powf(time_from_decay_start.as_secs_f64() * self.halving_rate);
            Some(decay_multiplier * release_multiplier)
        }
    }
}

/// A synthesiser that supports phase-offset modulation.
///
/// TODO: `set_frequency`, `set_start`
pub trait Pom<Data> {
    /// Samples the synthesiser. `global_time` represents the current time.
    ///
    /// When `None` is returned, this represents off, and it can be safely replaced with 0.0.
    fn sample(&mut self, data: &Data, global_time: Duration, phase_offset: f64) -> Option<f64>;
    /// Starts the synthesiser at the last global time.
    fn play(&mut self, frequency: f64, volume: f64);
    /// Stops the synthesiser immediately.
    fn cut(&mut self);
    /// Sets the synthesiser into the release section of its envelope.
    fn release(&mut self);
    /// Clones the synthesiser into a boxed trait object.
    fn box_clone(&self) -> Box<dyn Pom<Data>>;
}

/// Constants for [`Operator`]s to tweak their behaviour.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Binary)]
pub struct OperatorModifiers {
    pub frequency_multiplier: f64,
    pub volume_multiplier: f64,
    pub constant_phase_offset: f64,
}
impl Default for OperatorModifiers {
    fn default() -> Self {
        Self {
            frequency_multiplier: 1.0,
            volume_multiplier: 1.0,
            constant_phase_offset: 0.0,
        }
    }
}

/// A synthesiser that produces an enveloped waveform at a set frequency.
#[derive(Clone, Debug, Default, PartialEq, PartialOrd, Binary)]
pub struct Operator {
    pub waveform: Waveform,
    pub envelope: Envelope,
    pub modifiers: OperatorModifiers,

    pub start_time: Option<Option<Duration>>,
    pub stop_point: Option<Duration>,
    pub frequency: f64,
    pub peak_volume: f64,
    pub last_global_time: Option<Duration>,
    pub current_waveform_period: Period,
}
impl Operator {
    pub fn new(waveform: Waveform, envelope: Envelope, modifiers: OperatorModifiers) -> Self {
        Self {
            waveform,
            envelope,
            modifiers,
            frequency: 0.0,
            peak_volume: 0.0,
            start_time: None,
            stop_point: None,
            last_global_time: None,
            current_waveform_period: Period::ZERO,
        }
    }
}
impl Pom<SampleBank> for Operator {
    fn sample(
        &mut self,
        data: &SampleBank,
        global_time: Duration,
        phase_offset: f64,
    ) -> Option<f64> {
        let delta_time =
            global_time.saturating_sub(*self.last_global_time.get_or_insert(global_time));
        self.last_global_time = Some(global_time);

        let Some(start_time) = self.start_time else {
            return None; // note is off
        };
        let start_time = match start_time {
            Some(time) => time,
            None => {
                self.start_time = Some(Some(global_time));
                global_time
            }
        };
        if global_time < start_time {
            return None; // note hasnt started
        }

        let note_time = global_time.saturating_sub(start_time);
        let Some(envelope_multiplier) = self.envelope.sample_volume(note_time, self.stop_point)
        else {
            return None; // note has ended
        };

        // println!("{self:?} {} {}", self.frequency, self.peak_volume);

        // at
        self.current_waveform_period =
            self.current_waveform_period
                .saturating_add(time::duration_saturating_mul_f64(
                    delta_time,
                    self.frequency,
                ));
        Some(
            self.waveform.sample(
                data,
                self.current_waveform_period,
                phase_offset + self.modifiers.constant_phase_offset,
            ) * envelope_multiplier
                * self.peak_volume,
        )
    }

    fn play(&mut self, frequency: f64, volume: f64) {
        self.peak_volume = volume * self.modifiers.volume_multiplier;
        self.frequency = frequency * self.modifiers.frequency_multiplier;
        self.start_time = Some(self.last_global_time);
        self.stop_point = None;
    }
    fn release(&mut self) {
        self.stop_point
            .get_or_insert(self.last_global_time.unwrap_or(Duration::ZERO));
    }
    fn cut(&mut self) {
        self.start_time = None;
        self.stop_point = None;
    }
    fn box_clone(&self) -> Box<dyn Pom<SampleBank>> {
        Box::new(self.clone())
    }
}

/// A combinator
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum CombinatorType {
    Modulate,
    Sum,
}
pub struct Combinator<Data> {
    pub synths: Vec<Box<dyn Pom<Data>>>,
    pub ty: CombinatorType,
}
impl<Data: 'static> Pom<Data> for Combinator<Data> {
    fn sample(&mut self, data: &Data, global_time: Duration, phase_offset: f64) -> Option<f64> {
        match self.ty {
            CombinatorType::Modulate => {
                let mut carry = Some(phase_offset);
                for op in &mut self.synths {
                    carry = op.sample(data, global_time, carry.unwrap_or_default());
                }
                carry
            }
            CombinatorType::Sum => Some(
                self.synths
                    .iter_mut()
                    .map(|op| {
                        op.sample(data, global_time, phase_offset)
                            .unwrap_or_default()
                    })
                    .sum(),
            ),
        }
    }

    fn play(&mut self, frequency: f64, volume: f64) {
        self.synths
            .iter_mut()
            .for_each(|op| op.play(frequency, volume));
    }
    fn cut(&mut self) {
        self.synths.iter_mut().for_each(|op| op.cut());
    }
    fn release(&mut self) {
        self.synths.iter_mut().for_each(|op| op.release());
    }
    fn box_clone(&self) -> Box<dyn Pom<Data>> {
        Box::new(Self {
            synths: self.synths.iter().map(|op| op.box_clone()).collect(),
            ..*self
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Binary)]
pub enum StackInstruction {
    /// Pushes a constant value.
    Constant(f64),
    /// Pushes the input (given as `phase_offset`).
    InputPhaseOffset,

    /// Pops a value,
    /// then samples the given operator with that value as a phase offset,
    /// pushing it back onto the stack.
    Sample(u64),
    /// Pops and computes the sum of the top two values of the stack.
    Add,
    /// Duplicates the top value of the stack.
    Dupe,
}
/// Combines operators together using a simple stack-based executor.
///
/// This allows you to freely modify operators and instructions without reconstruction,
/// as you would need to do otherwise with more generic synths.
#[derive(Debug, Clone, PartialEq, PartialOrd, Binary)]
pub struct Stacker {
    pub operators: Vec<Operator>,
    pub instructions: Vec<StackInstruction>,
}
impl Stacker {
    pub fn chain(operators: Vec<Operator>) -> Self {
        let mut instructions = vec![StackInstruction::InputPhaseOffset];
        for i in (0..operators.len()).rev() {
            instructions.push(StackInstruction::Sample(i as u64));
        }
        Self {
            operators,
            instructions,
        }
    }
    pub fn add(operators: Vec<Operator>) -> Self {
        let mut instructions = vec![];
        if operators.is_empty() {
            instructions.push(StackInstruction::InputPhaseOffset);
        } else {
            for i in 0..operators.len() {
                instructions.push(StackInstruction::Constant(0.0));
                instructions.push(StackInstruction::Sample(i as u64));
                instructions.push(StackInstruction::Add);
            }
        }
        Self {
            operators,
            instructions,
        }
    }
}
impl Pom<SampleBank> for Stacker {
    fn sample(
        &mut self,
        data: &SampleBank,
        global_time: Duration,
        phase_offset: f64,
    ) -> Option<f64> {
        let mut stack = vec![];
        for instruction in &self.instructions {
            match instruction {
                StackInstruction::Constant(constant) => stack.push(*constant),
                StackInstruction::InputPhaseOffset => stack.push(phase_offset),
                StackInstruction::Sample(op) => {
                    let phase_offset = stack.pop().unwrap_or(0.0);
                    let Some(op) = self.operators.get_mut(*op as usize) else {
                        stack.push(0.0);
                        break;
                    };
                    stack.push(op.sample(data, global_time, phase_offset).unwrap_or(0.0));
                }
                StackInstruction::Add => {
                    let lhs = stack.pop().unwrap_or(0.0);
                    let rhs = stack.pop().unwrap_or(0.0);
                    stack.push(lhs + rhs);
                }
                StackInstruction::Dupe => stack.push(stack.last().copied().unwrap_or(0.0)),
            }
        }
        stack.pop()
    }

    fn play(&mut self, frequency: f64, volume: f64) {
        self.operators
            .iter_mut()
            .for_each(|op| op.play(frequency, volume));
    }
    fn cut(&mut self) {
        self.operators.iter_mut().for_each(|op| op.cut());
    }
    fn release(&mut self) {
        self.operators.iter_mut().for_each(|op| op.release());
    }
    fn box_clone(&self) -> Box<dyn Pom<SampleBank>> {
        Box::new(self.clone())
    }
}
