use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use infinitedsp_core::core::audio_param::AudioParam;
use infinitedsp_core::core::channels::{DualMono, Mono, Stereo};
use infinitedsp_core::core::dsp_chain::DspChain;
use infinitedsp_core::core::parallel_mixer::ParallelMixer;
use infinitedsp_core::core::parameter::Parameter;
use infinitedsp_core::core::summing_mixer::SummingMixer;
use infinitedsp_core::effects::filter::predictive_ladder::PredictiveLadderFilter;
use infinitedsp_core::effects::time::delay::Delay;
use infinitedsp_core::effects::time::reverb::Reverb;
use infinitedsp_core::effects::utility::gain::Gain;
use infinitedsp_core::effects::utility::offset::Offset;
use infinitedsp_core::effects::utility::stereo_widener::StereoWidener;
use infinitedsp_core::synthesis::envelope::Adsr;
use infinitedsp_core::synthesis::oscillator::{Oscillator, Waveform as CoreWaveform};
use infinitedsp_core::FrameProcessor;
use std::sync::{Arc, Mutex};

use crate::dsp_utils::Sum;
use crate::fast_lfo::{FastLfo, FastLfoWaveform};
use crate::protocol::{LfoWaveform, OscSettings, Preset, Waveform};

// --- Helpers ---

#[derive(Clone)]
struct SharedValue {
    value: Arc<Mutex<f32>>,
}

impl SharedValue {
    fn new(val: f32) -> Self {
        Self {
            value: Arc::new(Mutex::new(val)),
        }
    }

    fn set(&self, val: f32) {
        *self.value.lock().unwrap() = val;
    }
}

impl FrameProcessor<Mono> for SharedValue {
    fn process(&mut self, buffer: &mut [f32], _frame_index: u64) {
        let val = *self.value.lock().unwrap();
        for sample in buffer.iter_mut() {
            *sample = val;
        }
    }
    fn set_sample_rate(&mut self, _sample_rate: f32) {}
    fn reset(&mut self) {}
    fn latency_samples(&self) -> u32 {
        0
    }
    fn name(&self) -> &str {
        "SharedValue"
    }
    fn visualize(&self, _indent: usize) -> String {
        "SharedValue".into()
    }
}

// --- Portamento Frequency Control ---

struct PortamentoState {
    current_freq: f32,
    counter: usize,
}

#[derive(Clone)]
struct PortamentoFreq {
    target_freq: Arc<Mutex<f32>>,
    portamento_amount: Parameter,
    state: Arc<Mutex<PortamentoState>>,
}

impl PortamentoFreq {
    fn new(start_freq: f32) -> Self {
        Self {
            target_freq: Arc::new(Mutex::new(start_freq)),
            portamento_amount: Parameter::new(0.0),
            state: Arc::new(Mutex::new(PortamentoState {
                current_freq: start_freq,
                counter: 0,
            })),
        }
    }

    fn set_target(&self, freq: f32) {
        *self.target_freq.lock().unwrap() = freq;
    }

    fn set_portamento(&self, amount: f32) {
        self.portamento_amount.set(amount);
    }
}

impl FrameProcessor<Mono> for PortamentoFreq {
    fn process(&mut self, buffer: &mut [f32], _frame_index: u64) {
        let target = *self.target_freq.lock().unwrap();
        let amount = self.portamento_amount.get();
        let mut state = self.state.lock().unwrap();

        // Exponential Glide (Constant Rate / Filter Glide)
        // factor = 1.0 - P
        let p = amount.clamp(0.0, 0.999);
        let factor = 1.0 - p;

        for sample in buffer.iter_mut() {
            // Update every 32 samples (Control Rate)
            if state.counter % 32 == 0 {
                if p > 0.0 {
                    let diff = target - state.current_freq;
                    // Snap to target if close enough
                    if diff.abs() < 0.1 {
                        state.current_freq = target;
                    } else {
                        state.current_freq += diff * factor;
                    }
                } else {
                    state.current_freq = target;
                }
            }

            *sample = state.current_freq;
            state.counter += 1;
        }
    }

    fn set_sample_rate(&mut self, _sample_rate: f32) {}
    fn reset(&mut self) {}
    fn latency_samples(&self) -> u32 {
        0
    }
    fn name(&self) -> &str {
        "PortamentoFreq"
    }
    fn visualize(&self, _indent: usize) -> String {
        "PortamentoFreq".into()
    }
}

// --- Live Parameters ---

#[derive(Clone)]
struct LiveParams {
    osc1_level: Parameter,
    osc1_octave: Parameter,
    osc1_detune: Parameter,
    osc2_level: Parameter,
    osc2_octave: Parameter,
    osc2_detune: Parameter,
    osc3_level: Parameter,
    osc3_octave: Parameter,
    osc3_detune: Parameter,
    noise_level: Parameter,
    cutoff: Parameter,
    resonance: Parameter,
    filter_env_amt: Parameter,
    filter_attack: Parameter,
    filter_decay: Parameter,
    filter_sustain: Parameter,
    filter_release: Parameter,
    amp_attack: Parameter,
    amp_decay: Parameter,
    amp_sustain: Parameter,
    amp_release: Parameter,
    delay_time: Parameter,
    delay_feedback: Parameter,
    delay_mix: Parameter,
    reverb_size: Parameter,
    reverb_damping: Parameter,
    reverb_mix: Parameter,
    lfo_freq: Parameter,
    lfo_vib_amt: Parameter,
    lfo_filt_amt: Parameter,
    last_struct_hash: u64,
}

impl LiveParams {
    fn new() -> Self {
        Self {
            osc1_level: Parameter::new(1.0),
            osc1_octave: Parameter::new(0.0),
            osc1_detune: Parameter::new(0.0),
            osc2_level: Parameter::new(0.0),
            osc2_octave: Parameter::new(0.0),
            osc2_detune: Parameter::new(0.0),
            osc3_level: Parameter::new(0.0),
            osc3_octave: Parameter::new(0.0),
            osc3_detune: Parameter::new(0.0),
            noise_level: Parameter::new(0.0),
            cutoff: Parameter::new(20000.0),
            resonance: Parameter::new(0.0),
            filter_env_amt: Parameter::new(0.0),
            filter_attack: Parameter::new(0.0),
            filter_decay: Parameter::new(0.0),
            filter_sustain: Parameter::new(1.0),
            filter_release: Parameter::new(0.0),
            amp_attack: Parameter::new(0.01),
            amp_decay: Parameter::new(0.1),
            amp_sustain: Parameter::new(1.0),
            amp_release: Parameter::new(0.1),
            delay_time: Parameter::new(0.5),
            delay_feedback: Parameter::new(0.0),
            delay_mix: Parameter::new(0.0),
            reverb_size: Parameter::new(0.5),
            reverb_damping: Parameter::new(0.5),
            reverb_mix: Parameter::new(0.0),
            lfo_freq: Parameter::new(1.0),
            lfo_vib_amt: Parameter::new(0.0),
            lfo_filt_amt: Parameter::new(0.0),
            last_struct_hash: 0,
        }
    }

    fn update(&mut self, p: &Preset) -> bool {
        self.osc1_level.set(p.osc1.level);
        self.osc1_octave.set(p.osc1.octave);
        self.osc1_detune.set(p.osc1.detune);
        self.osc2_level.set(p.osc2.level);
        self.osc2_octave.set(p.osc2.octave);
        self.osc2_detune.set(p.osc2.detune);
        self.osc3_level.set(p.osc3.level);
        self.osc3_octave.set(p.osc3.octave);
        self.osc3_detune.set(p.osc3.detune);
        self.noise_level.set(p.noise);
        self.cutoff.set(p.filter.cutoff);
        self.resonance.set(p.filter.resonance);
        self.filter_env_amt.set(p.filter.env_amt);
        self.filter_attack.set(p.filter.attack);
        self.filter_decay.set(p.filter.decay);
        self.filter_sustain.set(p.filter.sustain);
        self.filter_release.set(p.filter.release);
        self.amp_attack.set(p.amp.attack);
        self.amp_decay.set(p.amp.decay);
        self.amp_sustain.set(p.amp.sustain);
        self.amp_release.set(p.amp.release);
        self.delay_time.set(p.delay.time);
        self.delay_feedback.set(p.delay.feedback);
        self.delay_mix.set(p.delay.mix);
        self.reverb_size.set(p.reverb.size);
        self.reverb_damping.set(p.reverb.damping);
        self.reverb_mix.set(p.reverb.mix);
        self.lfo_freq.set(p.lfo.freq);
        self.lfo_vib_amt.set(p.lfo.vib_amt);
        self.lfo_filt_amt.set(p.lfo.filt_amt);

        let mut hash = 0u64;
        hash = hash.wrapping_add(p.osc1.waveform as u64);
        hash = hash.wrapping_add((p.osc2.waveform as u64) << 4);
        hash = hash.wrapping_add((p.osc3.waveform as u64) << 8);
        hash = hash.wrapping_add(if p.osc1.vibrato { 1 } else { 0 } << 12);
        hash = hash.wrapping_add(if p.osc2.vibrato { 1 } else { 0 } << 13);
        hash = hash.wrapping_add(if p.osc3.vibrato { 1 } else { 0 } << 14);
        hash = hash.wrapping_add(if p.lfo_enabled { 1 } else { 0 } << 15);
        hash = hash.wrapping_add((p.lfo.waveform as u64) << 16);
        hash = hash.wrapping_add(if p.delay.enabled { 1 } else { 0 } << 20);
        hash = hash.wrapping_add(if p.reverb.enabled { 1 } else { 0 } << 21);

        let changed = hash != self.last_struct_hash;
        self.last_struct_hash = hash;
        changed
    }
}

// --- Voice Construction ---

fn map_waveform(w: Waveform) -> CoreWaveform {
    match w {
        Waveform::Sine => CoreWaveform::Sine,
        Waveform::Triangle => CoreWaveform::Triangle,
        Waveform::Saw => CoreWaveform::Saw,
        Waveform::Square => CoreWaveform::Square,
        Waveform::Noise => CoreWaveform::WhiteNoise,
    }
}

fn map_lfo_waveform(w: LfoWaveform) -> FastLfoWaveform {
    match w {
        LfoWaveform::Sine => FastLfoWaveform::Sine,
        LfoWaveform::Triangle => FastLfoWaveform::Triangle,
        LfoWaveform::Saw => FastLfoWaveform::Saw,
        LfoWaveform::Square => FastLfoWaveform::Square,
    }
}

fn create_pitch(
    params: &OscSettings,
    detune_param: Parameter,
    vibrato_enabled: bool,
    base_freq: impl FrameProcessor<Mono> + Send + 'static + Clone,
    vib: Option<FastLfo>,
    sample_rate: f32,
) -> AudioParam {
    let mut chain = DspChain::new(base_freq, sample_rate);

    if params.octave != 0.0 {
        let mult = libm::powf(2.0, params.octave);
        chain = chain.and(Gain::new_fixed(mult));
    }

    // Dynamic Detune
    chain = chain.and(Offset::new_param(AudioParam::Linked(detune_param)));

    if vibrato_enabled {
        if let Some(v) = vib {
            chain = chain.and(Sum::new(AudioParam::Dynamic(Box::new(v))));
        }
    }

    AudioParam::Dynamic(Box::new(chain))
}

pub struct AudioManager {
    _stream: cpal::Stream,
    // Fields kept alive by Arc clones in closures, but we hold them here to prevent drop
    _freq_ctrl: PortamentoFreq,
    _gate_ctrl: SharedValue,
    params: LiveParams,
    sender: crossbeam_channel::Sender<AudioCommand>,
    pub scope_buffer: Arc<Mutex<Vec<f32>>>,
}

enum AudioCommand {
    NoteOn(f32),
    NoteOff,
    UpdatePreset(Box<Preset>),
    RebuildVoice(Box<Preset>),
}

impl AudioManager {
    pub fn new() -> Result<Self, anyhow::Error> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(anyhow::anyhow!("No output device"))?;

        // Get default config to determine channels
        let default_config = device.default_output_config()?;
        let channels = default_config.channels() as usize;
        let config = default_config.config();
        let sample_rate = config.sample_rate.0 as f32;

        let (tx, rx) = crossbeam_channel::bounded(16);

        let current_preset = Box::new(Preset::default());

        let freq_ctrl = PortamentoFreq::new(440.0);
        let gate_ctrl = SharedValue::new(0.0);
        let mut params = LiveParams::new();

        let freq_ctrl_clone = freq_ctrl.clone();
        let gate_ctrl_clone = gate_ctrl.clone();
        let params_clone = params.clone();

        params.update(&current_preset);

        let mut voice: Option<Box<dyn FrameProcessor<Stereo> + Send>> = Some(build_voice(
            &current_preset,
            &params_clone,
            sample_rate,
            freq_ctrl_clone.clone(),
            gate_ctrl_clone.clone(),
        ));

        let scope_buffer = Arc::new(Mutex::new(vec![0.0; 1024]));
        let scope_buffer_clone = scope_buffer.clone();

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        AudioCommand::UpdatePreset(p) => {
                            freq_ctrl_clone.set_portamento(p.portamento);
                            // params_clone is updated via shared atomics by main thread
                        }
                        AudioCommand::RebuildVoice(p) => {
                            freq_ctrl_clone.set_portamento(p.portamento);
                            let new_v = build_voice(
                                &p,
                                &params_clone,
                                sample_rate,
                                freq_ctrl_clone.clone(),
                                gate_ctrl_clone.clone(),
                            );
                            voice = Some(new_v);
                        }
                        AudioCommand::NoteOn(freq) => {
                            freq_ctrl_clone.set_target(freq);
                            gate_ctrl_clone.set(1.0);
                        }
                        AudioCommand::NoteOff => {
                            gate_ctrl_clone.set(0.0);
                        }
                    }
                }

                if let Some(v) = &mut voice {
                    if channels == 2 {
                        v.process(data, 0);
                    } else {
                        for sample in data.iter_mut() {
                            *sample = 0.0;
                        }
                    }
                } else {
                    for sample in data.iter_mut() {
                        *sample = 0.0;
                    }
                }

                // Copy to scope buffer (rolling buffer)
                if let Ok(mut scope) = scope_buffer_clone.try_lock() {
                    let frames = data.len() / channels;
                    let buffer_len = scope.len();

                    if frames >= buffer_len {
                        // New data fills the entire buffer
                        for i in 0..buffer_len {
                            // Take last 'buffer_len' frames from data
                            let offset = frames - buffer_len;
                            scope[i] = data[(offset + i) * channels]; // Take first channel
                        }
                    } else {
                        // Shift existing data to the left
                        scope.copy_within(frames.., 0);

                        // Append new data at the end
                        let start_index = buffer_len - frames;
                        for i in 0..frames {
                            scope[start_index + i] = data[i * channels]; // Take first channel
                        }
                    }
                }
            },
            |err| eprintln!("Stream error: {}", err),
            None,
        )?;

        stream.play()?;

        Ok(Self {
            _stream: stream,
            _freq_ctrl: freq_ctrl,
            _gate_ctrl: gate_ctrl,
            params,
            sender: tx,
            scope_buffer,
        })
    }

    pub fn note_on(&self, note: u8) {
        let freq = 440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0);
        let _ = self.sender.send(AudioCommand::NoteOn(freq));
    }

    pub fn note_off(&self) {
        let _ = self.sender.send(AudioCommand::NoteOff);
    }

    pub fn update_preset(&mut self, preset: &Preset) {
        let struct_changed = self.params.update(preset);

        if struct_changed {
            let _ = self
                .sender
                .send(AudioCommand::RebuildVoice(Box::new(preset.clone())));
        } else {
            let _ = self
                .sender
                .send(AudioCommand::UpdatePreset(Box::new(preset.clone())));
        }
    }
}

fn build_voice(
    preset: &Preset,
    params: &LiveParams,
    sample_rate: f32,
    freq_ctrl: PortamentoFreq,
    gate_ctrl: SharedValue,
) -> Box<dyn FrameProcessor<Stereo> + Send> {
    let (vibrato_node, filter_lfo_node) = if preset.lfo_enabled {
        let p = &preset.lfo;
        let mut lfo_vib = FastLfo::new(p.freq, map_lfo_waveform(p.waveform), sample_rate);
        lfo_vib.set_range(-p.vib_amt, p.vib_amt);

        let mut lfo_filt = FastLfo::new(p.freq, map_lfo_waveform(p.waveform), sample_rate);
        lfo_filt.set_range(-p.filt_amt, p.filt_amt);

        (Some(lfo_vib), Some(lfo_filt))
    } else {
        (None, None)
    };

    let clone_lfo = |lfo: &Option<FastLfo>| -> Option<FastLfo> {
        lfo.as_ref().map(|l| {
            let mut new_lfo = FastLfo::new(l.get_frequency(), l.get_waveform(), sample_rate);
            new_lfo.set_range(l.get_min(), l.get_max());
            new_lfo
        })
    };

    let osc1_vib = clone_lfo(&vibrato_node);
    let osc2_vib = clone_lfo(&vibrato_node);
    let osc3_vib = clone_lfo(&vibrato_node);

    let osc1_node = Oscillator::new(
        create_pitch(
            &preset.osc1,
            params.osc1_detune.clone(),
            preset.osc1.vibrato,
            freq_ctrl.clone(),
            osc1_vib,
            sample_rate,
        ),
        map_waveform(preset.osc1.waveform),
    );
    let osc2_node = Oscillator::new(
        create_pitch(
            &preset.osc2,
            params.osc2_detune.clone(),
            preset.osc2.vibrato,
            freq_ctrl.clone(),
            osc2_vib,
            sample_rate,
        ),
        map_waveform(preset.osc2.waveform),
    );
    let osc3_node = Oscillator::new(
        create_pitch(
            &preset.osc3,
            params.osc3_detune.clone(),
            preset.osc3.vibrato,
            freq_ctrl.clone(),
            osc3_vib,
            sample_rate,
        ),
        map_waveform(preset.osc3.waveform),
    );
    let noise_node = Oscillator::new(AudioParam::Static(0.0), CoreWaveform::WhiteNoise);

    let osc1_gained = DspChain::new(osc1_node, sample_rate)
        .and(Gain::new(AudioParam::Linked(params.osc1_level.clone())));
    let osc2_gained = DspChain::new(osc2_node, sample_rate)
        .and(Gain::new(AudioParam::Linked(params.osc2_level.clone())));
    let osc3_gained = DspChain::new(osc3_node, sample_rate)
        .and(Gain::new(AudioParam::Linked(params.osc3_level.clone())));
    let noise_gained = DspChain::new(noise_node, sample_rate)
        .and(Gain::new(AudioParam::Linked(params.noise_level.clone())));

    let mixer = SummingMixer::new(vec![
        Box::new(osc1_gained),
        Box::new(osc2_gained),
        Box::new(osc3_gained),
        Box::new(noise_gained),
    ]);

    let filter_env = Adsr::new(
        AudioParam::Dynamic(Box::new(gate_ctrl.clone())),
        AudioParam::Linked(params.filter_attack.clone()),
        AudioParam::Linked(params.filter_decay.clone()),
        AudioParam::Linked(params.filter_sustain.clone()),
        AudioParam::Linked(params.filter_release.clone()),
    );

    let mut cutoff_mod_chain = DspChain::new(SharedValue::new(0.0), sample_rate)
        .and(Offset::new_param(AudioParam::Linked(params.cutoff.clone())))
        .and(Sum::new(AudioParam::Dynamic(Box::new(
            DspChain::new(filter_env, sample_rate)
                .and(Gain::new(AudioParam::Linked(params.filter_env_amt.clone()))),
        ))));

    if let Some(lfo) = filter_lfo_node {
        let lfo_chain = DspChain::new(lfo, sample_rate)
            .and(Gain::new(AudioParam::Linked(params.lfo_filt_amt.clone())));
        cutoff_mod_chain = cutoff_mod_chain.and(Sum::new(AudioParam::Dynamic(Box::new(lfo_chain))));
    }

    let filter_node = PredictiveLadderFilter::new(
        AudioParam::Dynamic(Box::new(cutoff_mod_chain)),
        AudioParam::Linked(params.resonance.clone()),
    );

    let amp_env = Adsr::new(
        AudioParam::Dynamic(Box::new(gate_ctrl)),
        AudioParam::Linked(params.amp_attack.clone()),
        AudioParam::Linked(params.amp_decay.clone()),
        AudioParam::Linked(params.amp_sustain.clone()),
        AudioParam::Linked(params.amp_release.clone()),
    );

    let vca = Gain::new(AudioParam::Dynamic(Box::new(amp_env)));

    let voice = DspChain::new(mixer, sample_rate).and(filter_node).and(vca);

    let mut chain: Box<dyn FrameProcessor<Stereo> + Send> = Box::new(voice.to_stereo());

    if preset.delay.enabled {
        let d = &preset.delay;
        let time_l = d.time;
        let time_r = d.time * 1.15;

        let delay_l = Delay::new(
            2.0,
            AudioParam::Static(time_l),
            AudioParam::Linked(params.delay_feedback.clone()),
            AudioParam::Linked(params.delay_mix.clone()),
        );
        let delay_r = Delay::new(
            2.0,
            AudioParam::Static(time_r),
            AudioParam::Linked(params.delay_feedback.clone()),
            AudioParam::Linked(params.delay_mix.clone()),
        );

        chain = Box::new(
            DspChain::new(chain, sample_rate)
                .and(ParallelMixer::new(1.0, DualMono::new(delay_l, delay_r))),
        );
    }

    if preset.reverb.enabled {
        let reverb = Reverb::new_with_params(
            AudioParam::Linked(params.reverb_size.clone()),
            AudioParam::Linked(params.reverb_damping.clone()),
            0,
        );
        chain = Box::new(
            DspChain::new(chain, sample_rate)
                .and_mix_param(AudioParam::Linked(params.reverb_mix.clone()), reverb),
        );
    }

    let widener = StereoWidener::new(AudioParam::Static(1.5));
    chain = Box::new(
        DspChain::new(chain, sample_rate)
            .and(widener)
            .and(Gain::new_fixed(0.5)),
    );

    chain
}
