// ---------- TYPES ----------

#include <cstddef>
#include <cstdint>

/// An opaque type representing a sampleable, phase offset-modulated (POM)
/// synthesiser.
typedef struct Pom Pom;
/// An opaque type representing a bank of samples.
typedef struct PomPCMBank PomPCMBank;

/// A duration.
///
/// Matches Rust's `Duration`, as that is what this converts directly into.
typedef struct PomDuration {
    uint64_t seconds;
    uint32_t nanoseconds;
} PomDuration;

/// A waveform shape for an operator.
typedef int PomWaveformType;
#define POM_WAVEFORM_TYPE_SINE 0
#define POM_WAVEFORM_TYPE_PULSE 1
#define POM_WAVEFORM_TYPE_TRIANGLE 2
#define POM_WAVEFORM_TYPE_SAWTOOTH 3
#define POM_WAVEFORM_TYPE_INVERTED_SAWTOOTH 4
#define POM_WAVEFORM_TYPE_PCM 5
#define POM_WAVEFORM_TYPE_CONSTANT 6

/// An identifier for a sample in a sample bank.
typedef uint64_t PomSampleID;

/// Waveform settings for an operator.
typedef struct PomWaveform {
    PomWaveformType type;
    union {
        double duty_cycle;
        double constant_offset;
        PomSampleID sample_id;
    };
} PomWaveform;

/// An envelope for an operator.
typedef struct PomEnvelope {
    PomDuration attack_time;
    double halving_rate;
    PomDuration release_time;
} PomEnvelope;

/// Modifiers that are applied to an operator.
typedef struct PomModifiers {
    double frequency_multiplier;
    double volume_multiplier;
    double constant_phase_offset;
} PomModifiers;

/// Settings for creating an operator.
typedef struct PomOperatorSettings {
    PomWaveform waveform;
    PomEnvelope envelope;
    PomModifiers modifiers;
} PomOperatorSettings;

/// Settings for creating an operator.
typedef struct PomPCMSampleSettings {
    double samples_per_period;
    PomDuration loop_point;
    PomDuration loop_duration;
} PomPCMSampleSettings;

/// An algorithm for a combinator.
typedef int PomCombinatorType;
#define POM_COMBINATOR_TYPE_SUM 0
#define POM_COMBINATOR_TYPE_MODULATE 1

/// A result type.
typedef int PomResult;
#define POM_SUCCESS 0
#define POM_FAIL_INVALID_INPUT 1

/// A type that represents a PCM sample format.
typedef int PomSampleFormat;
#define POM_SAMPLE_FORMAT_U8 0
#define POM_SAMPLE_FORMAT_I16 1
#define POM_SAMPLE_FORMAT_I32 2
#define POM_SAMPLE_FORMAT_F32 3
#define POM_SAMPLE_FORMAT_F64 4

// ---------- CREATION ----------

/// Allocates a new operator. An operator is the most basic synthesiser; it
/// simply produces a waveform.
extern PomResult pom_create_operator(Pom** out, PomOperatorSettings settings);
/// Creates a modulation combinator, which modulates the phase offset of the
/// signal from `carrier` with the signal from `modulator`.
extern PomResult
pom_create_modulator(Pom** out, const Pom* modulator, const Pom* carrier);
/// Creates a summation combinator, which sums both signals together.
extern PomResult pom_create_summation(Pom** out, const Pom* a, const Pom* b);
/// Creates a combinator, with any number of synths, combined via the given
/// algorithm.
extern PomResult pom_create_combinator(
    Pom** out, const Pom* synths[], uint64_t synth_count, PomCombinatorType type
);
/// Clones an existing synthesiser.
extern PomResult pom_clone_synth(Pom** out, const Pom* source);

/// Creates a new, empty PCM bank.
extern PomResult pom_create_pcm_bank(PomPCMBank** out);
/// Clones an existing PCM bank.
extern PomResult pom_clone_pcm_bank(PomPCMBank** out, const PomPCMBank* source);

// ---------- STATE ----------

/// Marks a synthesiser as playing at its current position.
extern void pom_play(Pom* synth, double frequency, double volume);
/// Marks a synthesiser as releasing at its current position.
extern void pom_release(Pom* synth);
/// Hard stops a synthesiser.
extern void pom_cut(Pom* synth);

/// Adds a PCM sample to a PCM bank.
extern void pom_add_pcm(
    PomPCMBank* bank,
    void* pcm_data,
    uint64_t pcm_length,
    PomSampleFormat pcm_sample_format,
    PomSampleID identifier,
    PomPCMSampleSettings pcm_sample_settings
);

// ---------- SAMPLING ----------

/// Samples a synthesiser once, stepping it to the given current time.
extern double pom_sample(
    Pom* synth,
    const PomPCMBank* bank,
    PomDuration global_time,
    double input_phase_offset
);
/// Samples a synthesiser many times, filling an audio array.
/// The byte size of the data is `length` times the size of the sample format.
extern PomResult pom_fill(
    Pom* synth,
    const PomPCMBank* bank,
    PomDuration start_time,
    PomDuration sample_interval,
    void* data,
    uint64_t length,
    PomSampleFormat sample_format,
    double constant_phase_offset
);

// ---------- CLEANUP ----------

/// Destroys a synthesiser.
extern void pom_destroy_synth(Pom* object);
/// Destroys a PCM bank.
extern void pom_destroy_pcm_bank(PomPCMBank* bank);
