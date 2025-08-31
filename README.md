# Pommel
A phase-offset modulation synthesis library for audio generation, inspired by Yamaha's YM series and ESS's ESFM technology.

# Usage
## `Pom`
The heart of Pommel is the `Pom` trait. An object that implements this trait acts like a synthesiser. The `Data` generic parameter is used for any types of data that the synthesiser may need, for example a sample bank.

## 
The main types exported by this crate are as follows:
- **Operator**: This is hich produces a waveform whose amplitude is controlled by an envelope, and contains other parameters that are customised via `OperatorModifiers`. `Pom` synths can act as modulators. The `Stacker` synth can be used to 

This crate does not do playback. It only facilitates synthesis. The output of a `Pom` synth can be redirected into a PCM output stream. Outputs are generally in the range of [-1, 1].

## C FFI
Pommel exports a C FFI which, while ***not yet stable***, allows you to use Pommel from C code. `pommel.h` declares all C-exported functions. This interface is partially inspired by Vulkan's API, using construction information structures in some places.

The API is not yet complete; I am hoping to add functions to construct more complex waveform types such as `Thin`, `Cut`, and `Absolute`.

# Integration with Decent
The types within this crate can be serialised to binary streams with the help of my binary serde crate, Decent. This allows you to read/write structures to binary streams, which is useful for modules. Note that, as with Decent itself, ***stability is not guaranteed!*** This functionality is experimental, and is implemented here for use in other projects of mine.

# Technical Deep Dive
It's hard to use something when you don't know how it works, so let's dive into both frequency and phase-offset modulation.

## Frequency Modulation
Frequency modulation (FM) is a very common term in communications and audio. Alongside AM, it is a way to carry radio signals, and alongside wavetable, additive, and other synthesis methods, it also acts as a method for generating audio. Frequency modulation is where the frequency of some waveform (the carrier) is defined as a function of another waveform (the modulator). Commonly, these are sinusoids, but any waveform can theoretically be used.

### Operators
FM isn't limited to two waveforms. Any number of waveforms can be used. A single waveform generator in an FM synth is called an operator, and each has a set of parameters, commonly including a fixed frequency multiplier and an amplitude multiplier. The most common configuration is a four-op chain. The first operator modulates the second operator, the output of that modulates the third operator, and the output of *that* modulates the fourth, the output of which is used as audio output.

## Phase Offset Modulation
Instead of modulating the frequency of a signal, you can instead modulate the phase of the waveform. This is what I dub phase-offset modulation (POM). It is much easier to compute; the sampling function can be pure and non-monotonic. An example of a phase-offset modulated waveform is the simple modulation of a sine wave by a sine wave: `f(x) = sin(x + a sin(f x))`, where `a` is the amplitude and `f` is the frequency of the first operator.

## FM vs. POM
Although FM and POM differ in the parameter they modulate, they aren't so different. In fact, they're almost identical! FM technologies like ESFM and the YM series *actually compute audio using POM*!! But, it is still technically FM, and there is an explanation.

Say you have a 1Hz FM-modulatable wave, given by `f(x, m)`, where `x` is the input time and `m` is an added frequency (in hertz). If you wanted to pass the wave through as-is, you would call the function with `f(x, 0)`. If you wanted to add a constant 0.5Hz as a "modulator", you could use `f(x, 0.5)`, producing a 1.5Hz wave. Now let's do the same, but say `m` is a given phase offset. To pass the wave as-is, you would call `f(x, 0)`, similar to FM. But, to add a constant 0.5Hz, you would call `f(x, 0.5x)`. Note the addition of the `x`. We're sampling the wave, but to get an extra 0.5Hz out, we need to complete another one-half of a waveform cycle per second, which is represented by our `0.5x`.

What is `0.5` in FM becomes `0.5x` in POM. If you've taken calculus, you might've seen that `0.5` is the derivative of `0.5x`, which is the right conclusion. This is **exactly** what separates FM from POM: **The `m` parameter in POM is the integral of the `m` parameter in FM**. The reason FM chips can get away with using POM instead of FM is because they use sinusoids (a lot of them can *only* use sinusoids!), and the derivative of a sinusoid is *another sinusoid*; `sin(x)` in POM becomes `cos(x)` in FM!

# Checklist
- [ ] Examples
- [ ] Integrate with Cranelift for JIT-compiled synthesisers.
- [ ] A more complete C FFI, especially one that includes more complex waveform types.
