pub const SYSEX_START: u8 = 0xF0;
pub const SYSEX_END: u8 = 0xF7;
pub const MANUFACTURER_ID: u8 = 0x7D;
pub const MODEL_ID: u8 = 0x01;

// Commands
pub const CMD_DUMP_REQ: u8 = 0x01;
pub const CMD_WRITE_REQ: u8 = 0x02;
pub const CMD_WRITE_SUCCESS: u8 = 0x03;
pub const CMD_WRITE_ERROR: u8 = 0x04;

pub const MAGIC: u32 = 0x50445350;
pub const VERSION: u32 = 7;
pub const STORAGE_SIZE: usize = 4096;
pub const PRESET_SIZE: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Waveform {
    Sine = 0,
    Triangle = 1,
    Saw = 2,
    Square = 3,
    Noise = 4,
}

impl From<u32> for Waveform {
    fn from(val: u32) -> Self {
        match val {
            0 => Waveform::Sine,
            1 => Waveform::Triangle,
            2 => Waveform::Saw,
            3 => Waveform::Square,
            _ => Waveform::Noise,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[derive(Default)]
pub enum LfoWaveform {
    #[default]
    Sine = 0,
    Triangle = 1,
    Saw = 2,
    Square = 3,
}


impl From<u32> for LfoWaveform {
    fn from(val: u32) -> Self {
        match val {
            0 => LfoWaveform::Sine,
            1 => LfoWaveform::Triangle,
            2 => LfoWaveform::Saw,
            _ => LfoWaveform::Square,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OscSettings {
    pub waveform: Waveform,
    pub level: f32,
    pub octave: f32,
    pub detune: f32,
    pub vibrato: bool,
}

impl Default for OscSettings {
    fn default() -> Self {
        Self {
            waveform: Waveform::Saw,
            level: 1.0,
            octave: 0.0,
            detune: 0.0,
            vibrato: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FilterSettings {
    pub cutoff: f32,
    pub resonance: f32,
    pub env_amt: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for FilterSettings {
    fn default() -> Self {
        Self {
            cutoff: 20000.0,
            resonance: 0.0,
            env_amt: 0.0,
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnvSettings {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for EnvSettings {
    fn default() -> Self {
        Self {
            attack: 0.01,
            decay: 0.1,
            sustain: 1.0,
            release: 0.1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LfoSettings {
    pub freq: f32,
    pub waveform: LfoWaveform,
    pub vib_amt: f32,
    pub filt_amt: f32,
}

impl Default for LfoSettings {
    fn default() -> Self {
        Self {
            freq: 1.0,
            waveform: LfoWaveform::Sine,
            vib_amt: 0.0,
            filt_amt: 0.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DelaySettings {
    pub time: f32,
    pub feedback: f32,
    pub mix: f32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ReverbSettings {
    pub size: f32,
    pub damping: f32,
    pub mix: f32,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct Preset {
    pub name: String,
    pub osc1: OscSettings,
    pub osc2: OscSettings,
    pub osc3: OscSettings,
    pub noise: f32,
    pub portamento: f32,
    pub filter: FilterSettings,
    pub amp: EnvSettings,
    pub lfo_enabled: bool,
    pub lfo: LfoSettings,
    pub delay: DelaySettings,
    pub reverb: ReverbSettings,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            name: "Init Patch".to_string(),
            osc1: OscSettings::default(),
            osc2: OscSettings {
                level: 0.0,
                ..OscSettings::default()
            },
            osc3: OscSettings {
                level: 0.0,
                ..OscSettings::default()
            },
            noise: 0.0,
            portamento: 0.0,
            filter: FilterSettings::default(),
            amp: EnvSettings::default(),
            lfo_enabled: false,
            lfo: LfoSettings::default(),
            delay: DelaySettings::default(),
            reverb: ReverbSettings::default(),
        }
    }
}

fn write_f32(buf: &mut Vec<u8>, val: f32) {
    buf.extend_from_slice(&val.to_le_bytes());
}

fn read_f32(buf: &[u8], offset: &mut usize) -> f32 {
    let val = f32::from_le_bytes(buf[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    val
}

fn write_u32(buf: &mut Vec<u8>, val: u32) {
    buf.extend_from_slice(&val.to_le_bytes());
}

fn read_u32(buf: &[u8], offset: &mut usize) -> u32 {
    let val = u32::from_le_bytes(buf[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    val
}

impl Preset {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Name (32 bytes)
        let mut name_bytes = [0u8; 32];
        let bytes = self.name.as_bytes();
        let len = bytes.len().min(32);
        name_bytes[..len].copy_from_slice(&bytes[..len]);
        buf.extend_from_slice(&name_bytes);

        // Oscillators (20 bytes each)
        for osc in [&self.osc1, &self.osc2, &self.osc3] {
            write_u32(&mut buf, osc.waveform as u32);
            write_f32(&mut buf, osc.level);
            write_f32(&mut buf, osc.octave);
            write_f32(&mut buf, osc.detune);
            write_u32(&mut buf, if osc.vibrato { 1 } else { 0 });
        }

        // Noise (4 bytes)
        write_f32(&mut buf, self.noise);

        // Portamento (4 bytes)
        write_f32(&mut buf, self.portamento);

        // Filter (28 bytes)
        write_f32(&mut buf, self.filter.cutoff);
        write_f32(&mut buf, self.filter.resonance);
        write_f32(&mut buf, self.filter.env_amt);
        write_f32(&mut buf, self.filter.attack);
        write_f32(&mut buf, self.filter.decay);
        write_f32(&mut buf, self.filter.sustain);
        write_f32(&mut buf, self.filter.release);

        // Amp Env (16 bytes)
        write_f32(&mut buf, self.amp.attack);
        write_f32(&mut buf, self.amp.decay);
        write_f32(&mut buf, self.amp.sustain);
        write_f32(&mut buf, self.amp.release);

        // LFO Enabled (4 bytes)
        write_u32(&mut buf, if self.lfo_enabled { 1 } else { 0 });

        // LFO Settings (16 bytes)
        write_f32(&mut buf, self.lfo.freq);
        write_u32(&mut buf, self.lfo.waveform as u32);
        write_f32(&mut buf, self.lfo.vib_amt);
        write_f32(&mut buf, self.lfo.filt_amt);

        // Delay Settings (16 bytes)
        write_f32(&mut buf, self.delay.time);
        write_f32(&mut buf, self.delay.feedback);
        write_f32(&mut buf, self.delay.mix);
        write_u32(&mut buf, if self.delay.enabled { 1 } else { 0 });

        // Reverb Settings (16 bytes)
        write_f32(&mut buf, self.reverb.size);
        write_f32(&mut buf, self.reverb.damping);
        write_f32(&mut buf, self.reverb.mix);
        write_u32(&mut buf, if self.reverb.enabled { 1 } else { 0 });

        // Padding (4 bytes)
        write_u32(&mut buf, 0);

        buf
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        let mut offset = 0;

        // Name
        let name_bytes = &data[offset..offset + 32];
        let name = String::from_utf8_lossy(name_bytes)
            .trim_matches(char::from(0))
            .to_string();
        offset += 32;

        // Oscillators
        let mut oscs = Vec::new();
        for _ in 0..3 {
            let waveform = Waveform::from(read_u32(data, &mut offset));
            let level = read_f32(data, &mut offset);
            let octave = read_f32(data, &mut offset);
            let detune = read_f32(data, &mut offset);
            let vibrato = read_u32(data, &mut offset) != 0;

            oscs.push(OscSettings {
                waveform,
                level,
                octave,
                detune,
                vibrato,
            });
        }

        let noise = read_f32(data, &mut offset);
        let portamento = read_f32(data, &mut offset);

        // Filter
        let filter = FilterSettings {
            cutoff: read_f32(data, &mut offset),
            resonance: read_f32(data, &mut offset),
            env_amt: read_f32(data, &mut offset),
            attack: read_f32(data, &mut offset),
            decay: read_f32(data, &mut offset),
            sustain: read_f32(data, &mut offset),
            release: read_f32(data, &mut offset),
        };

        // Amp
        let amp = EnvSettings {
            attack: read_f32(data, &mut offset),
            decay: read_f32(data, &mut offset),
            sustain: read_f32(data, &mut offset),
            release: read_f32(data, &mut offset),
        };

        // LFO
        let lfo_enabled = read_u32(data, &mut offset) != 0;
        let lfo = LfoSettings {
            freq: read_f32(data, &mut offset),
            waveform: LfoWaveform::from(read_u32(data, &mut offset)),
            vib_amt: read_f32(data, &mut offset),
            filt_amt: read_f32(data, &mut offset),
        };

        // Delay
        let delay = DelaySettings {
            time: read_f32(data, &mut offset),
            feedback: read_f32(data, &mut offset),
            mix: read_f32(data, &mut offset),
            enabled: read_u32(data, &mut offset) != 0,
        };

        // Reverb
        let reverb = ReverbSettings {
            size: read_f32(data, &mut offset),
            damping: read_f32(data, &mut offset),
            mix: read_f32(data, &mut offset),
            enabled: read_u32(data, &mut offset) != 0,
        };

        // Padding
        let _padding = read_u32(data, &mut offset);

        Preset {
            name,
            osc1: oscs[0].clone(),
            osc2: oscs[1].clone(),
            osc3: oscs[2].clone(),
            noise,
            portamento,
            filter,
            amp,
            lfo_enabled,
            lfo,
            delay,
            reverb,
        }
    }
}

pub struct Storage {
    pub presets: Vec<Preset>,
}

impl Storage {
    pub fn to_sysex(&self) -> Vec<u8> {
        let mut raw_data = Vec::with_capacity(STORAGE_SIZE);

        // Header
        write_u32(&mut raw_data, MAGIC);
        write_u32(&mut raw_data, VERSION);
        write_u32(&mut raw_data, self.presets.len() as u32);
        write_u32(&mut raw_data, 0); // Padding

        // Presets
        for preset in &self.presets {
            raw_data.extend_from_slice(&preset.to_bytes());
        }

        // Fill rest with 0xFF (flash erased state) or 0x00
        while raw_data.len() < STORAGE_SIZE {
            raw_data.push(0);
        }

        // Nibbleize data (split each byte into two 4-bit nibbles)
        let mut nibble_data = Vec::with_capacity(STORAGE_SIZE * 2);
        for byte in raw_data {
            nibble_data.push((byte >> 4) & 0x0F); // High nibble
            nibble_data.push(byte & 0x0F); // Low nibble
        }

        // Construct SysEx message
        let mut msg = vec![SYSEX_START, MANUFACTURER_ID, MODEL_ID, CMD_WRITE_REQ];
        msg.extend_from_slice(&nibble_data);
        msg.push(SYSEX_END);

        msg
    }

    pub fn from_sysex(msg: &[u8]) -> Option<Self> {
        if msg.len() < 5 {
            return None;
        }
        if msg[0] != SYSEX_START || msg[msg.len() - 1] != SYSEX_END {
            return None;
        }
        if msg[1] != MANUFACTURER_ID || msg[2] != MODEL_ID {
            return None;
        }

        // Only parse if it is a Write Request / Dump Response
        if msg[3] != CMD_WRITE_REQ {
            return None;
        }

        let payload = &msg[4..msg.len() - 1];

        // Check if payload size matches expected nibbleized size
        if payload.len() != STORAGE_SIZE * 2 {
            return None;
        }

        // De-nibbleize (combine pairs of nibbles back to bytes)
        let mut data = Vec::with_capacity(STORAGE_SIZE);
        for chunk in payload.chunks(2) {
            if chunk.len() != 2 {
                return None;
            }
            let high = chunk[0];
            let low = chunk[1];
            // Reconstruct byte: (high << 4) | low
            data.push((high << 4) | (low & 0x0F));
        }

        let mut offset = 0;
        let magic = read_u32(&data, &mut offset);
        if magic != MAGIC {
            return None;
        }

        let _version = read_u32(&data, &mut offset);
        let num_presets = read_u32(&data, &mut offset);
        let _padding = read_u32(&data, &mut offset);

        let mut presets = Vec::new();

        for _ in 0..num_presets {
            let p = Preset::from_bytes(&data[offset..]);
            presets.push(p);
            offset += PRESET_SIZE;
        }

        Some(Storage { presets })
    }
}
