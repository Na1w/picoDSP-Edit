use eframe::egui;
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use rustfft::FftPlanner;
use std::error::Error;
use std::fs;
use std::sync::{Arc, Mutex};

mod protocol;
use protocol::*;

mod piano;
use piano::PianoWidget;

mod audio;
mod dsp_utils;
mod fast_lfo;
use audio::AudioManager;

mod ui;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PicoDSP Editor 1.0",
        options,
        Box::new(|_cc| Ok(Box::new(PicoEditApp::default()))),
    )
    .map_err(|e| e.into())
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum AudioMode {
    Local,
    Remote,
}

struct PicoEditApp {
    midi_in: Option<MidiInput>,
    midi_out: Option<MidiOutput>,
    in_port_name: Option<String>,
    out_port_name: Option<String>,

    audio_mode: AudioMode,

    conn_out: Option<MidiOutputConnection>,
    conn_in: Option<MidiInputConnection<()>>,

    storage: Arc<Mutex<Storage>>,
    current_preset_index: usize,
    last_preset_index: usize,
    status_msg: Arc<Mutex<String>>,

    active_notes: Vec<u8>,
    audio: Option<AudioManager>,
    fft_planner: Arc<Mutex<FftPlanner<f32>>>,
}

impl Default for PicoEditApp {
    fn default() -> Self {
        let mut midi_in = MidiInput::new("PicoEdit Input").unwrap();
        midi_in.ignore(Ignore::None);
        let midi_out = MidiOutput::new("PicoEdit Output").unwrap();

        let audio = match AudioManager::new() {
            Ok(a) => Some(a),
            Err(e) => {
                println!("Failed to initialize audio: {}", e);
                None
            }
        };

        let mut app = Self {
            midi_in: Some(midi_in),
            midi_out: Some(midi_out),
            in_port_name: None,
            out_port_name: None,
            audio_mode: AudioMode::Local,
            conn_out: None,
            conn_in: None,
            storage: Arc::new(Mutex::new(Storage {
                presets: vec![Preset::default()],
            })),
            current_preset_index: 0,
            last_preset_index: 0,
            status_msg: Arc::new(Mutex::new("Ready".to_string())),
            active_notes: Vec::new(),
            audio,
            fft_planner: Arc::new(Mutex::new(FftPlanner::new())),
        };

        app.auto_connect();
        app
    }
}

impl PicoEditApp {
    fn auto_connect(&mut self) {
        let target_in = self.find_port_by_name(true, "picodsp");
        let target_out = self.find_port_by_name(false, "picodsp");

        if let (Some(in_name), Some(out_name)) = (target_in, target_out) {
            self.in_port_name = Some(in_name.clone());
            self.out_port_name = Some(out_name.clone());
            self.connect_midi(&in_name, &out_name);
        }
    }

    fn find_port_by_name(&self, is_input: bool, pattern: &str) -> Option<String> {
        let pattern = pattern.to_lowercase();
        if is_input {
            if let Some(midi_in) = &self.midi_in {
                for port in midi_in.ports() {
                    if let Ok(name) = midi_in.port_name(&port) {
                        if name.to_lowercase().contains(&pattern) {
                            return Some(name);
                        }
                    }
                }
            }
        } else if let Some(midi_out) = &self.midi_out {
            for port in midi_out.ports() {
                if let Ok(name) = midi_out.port_name(&port) {
                    if name.to_lowercase().contains(&pattern) {
                        return Some(name);
                    }
                }
            }
        }
        None
    }

    fn refresh_midi(&mut self) {
        self.conn_in = None;
        self.conn_out = None;

        let mut midi_in = MidiInput::new("PicoEdit Input").unwrap();
        midi_in.ignore(Ignore::None);
        self.midi_in = Some(midi_in);

        self.midi_out = Some(MidiOutput::new("PicoEdit Output").unwrap());
        *self.status_msg.lock().unwrap() = "Ports refreshed".to_string();

        self.auto_connect();
    }

    fn connect_midi(&mut self, in_name: &str, out_name: &str) {
        self.conn_in = None;
        self.conn_out = None;

        self.ensure_midi_instances();

        if !self.connect_output(out_name) {
            return;
        }

        // Always connect input if available
        self.connect_input(in_name);

        if self.conn_out.is_some() {
            let in_status = if self.conn_in.is_some() {
                " + Input"
            } else {
                ""
            };
            *self.status_msg.lock().unwrap() = format!("Connected to Output{}", in_status);
            self.send_dump_request();
        }
    }

    fn ensure_midi_instances(&mut self) {
        if self.midi_in.is_none() {
            let mut midi_in = MidiInput::new("PicoEdit Input").unwrap();
            midi_in.ignore(Ignore::None);
            self.midi_in = Some(midi_in);
        }
        if self.midi_out.is_none() {
            self.midi_out = Some(MidiOutput::new("PicoEdit Output").unwrap());
        }
    }

    fn connect_output(&mut self, out_name: &str) -> bool {
        let midi_out = self.midi_out.take().unwrap();
        let out_ports = midi_out.ports();
        let out_port = out_ports
            .iter()
            .find(|p| midi_out.port_name(p).unwrap() == out_name);

        if let Some(op) = out_port {
            match midi_out.connect(op, "PicoEdit Out") {
                Ok(conn) => {
                    self.conn_out = Some(conn);
                    true
                }
                Err(e) => {
                    *self.status_msg.lock().unwrap() = format!("Error connecting output: {}", e);
                    self.midi_out = Some(MidiOutput::new("PicoEdit Output").unwrap());
                    false
                }
            }
        } else {
            *self.status_msg.lock().unwrap() = "Output port not found".to_string();
            self.midi_out = Some(midi_out);
            false
        }
    }

    fn connect_input(&mut self, in_name: &str) {
        let midi_in = self.midi_in.take().unwrap();
        let in_ports = midi_in.ports();
        let in_port = in_ports
            .iter()
            .find(|p| midi_in.port_name(p).unwrap() == in_name);

        if let Some(ip) = in_port {
            let storage_clone = self.storage.clone();
            let status_clone = self.status_msg.clone();
            let sysex_buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
            let buffer_clone = sysex_buffer.clone();

            match midi_in.connect(
                ip,
                "PicoEdit In",
                move |_stamp, message, _| {
                    Self::handle_midi_message(
                        message,
                        &buffer_clone,
                        &storage_clone,
                        &status_clone,
                    );
                },
                (),
            ) {
                Ok(conn) => {
                    self.conn_in = Some(conn);
                }
                Err(e) => {
                    *self.status_msg.lock().unwrap() = format!("Error connecting input: {}", e);
                    let mut midi_in = MidiInput::new("PicoEdit Input").unwrap();
                    midi_in.ignore(Ignore::None);
                    self.midi_in = Some(midi_in);
                }
            }
        } else {
            *self.status_msg.lock().unwrap() = "Input port not found".to_string();
            let mut midi_in = MidiInput::new("PicoEdit Input").unwrap();
            midi_in.ignore(Ignore::None);
            self.midi_in = Some(midi_in);
        }
    }

    fn handle_midi_message(
        message: &[u8],
        buffer_clone: &Arc<Mutex<Vec<u8>>>,
        storage_clone: &Arc<Mutex<Storage>>,
        status_clone: &Arc<Mutex<String>>,
    ) {
        /*if message.len() < 20 {
            println!("Rx Chunk ({} bytes): {:02X?}", message.len(), message);
        } else {
             println!("Rx Chunk ({} bytes): [First 10: {:02X?} ...]", message.len(), &message[0..10]);
        }*/

        let mut buffer = buffer_clone.lock().unwrap();

        if message.contains(&0xF0) {
            buffer.clear();
            if let Some(start) = message.iter().position(|&x| x == 0xF0) {
                buffer.extend_from_slice(&message[start..]);
            }
        } else if !buffer.is_empty() {
            buffer.extend_from_slice(message);
        }

        if let Some(&last) = buffer.last() {
            if last == 0xF7 {
                //     println!("Full SysEx received: {} bytes", buffer.len());
                Self::process_sysex(&buffer, storage_clone, status_clone);
                buffer.clear();
            }
        }
    }

    fn process_sysex(
        buffer: &[u8],
        storage_clone: &Arc<Mutex<Storage>>,
        status_clone: &Arc<Mutex<String>>,
    ) {
        if buffer.len() >= 5 && buffer[1] == MANUFACTURER_ID && buffer[2] == MODEL_ID {
            match buffer[3] {
                CMD_WRITE_REQ => match Storage::from_sysex(buffer) {
                    Some(new_storage) => {
                        let count = new_storage.presets.len();
                        *storage_clone.lock().unwrap() = new_storage;
                        *status_clone.lock().unwrap() = format!("Loaded {} presets!", count);
                    }
                    None => {
                        println!("Failed to parse SysEx via Storage::from_sysex!");
                        *status_clone.lock().unwrap() = "Failed to parse Dump!".to_string();
                    }
                },
                CMD_WRITE_SUCCESS => {
                    *status_clone.lock().unwrap() = "Save Successful!".to_string();
                }
                CMD_WRITE_ERROR => {
                    let err_code = if buffer.len() > 4 { buffer[4] } else { 0 };
                    println!("Received Write Error (NAK): Code {}", err_code);
                    *status_clone.lock().unwrap() =
                        format!("Save Failed! Error Code: {}", err_code);
                }
                _ => {
                    println!("Unknown Command: {:02X}", buffer[3]);
                }
            }
        } else {
            println!("Ignored SysEx (Wrong Header or too short): {:02X?}", buffer);
        }
    }

    fn send_dump_request(&mut self) {
        if let Some(conn) = &mut self.conn_out {
            let msg = [0xF0, MANUFACTURER_ID, MODEL_ID, CMD_DUMP_REQ, 0xF7];
            match conn.send(&msg) {
                Ok(_) => {
                    *self.status_msg.lock().unwrap() = "Sent Dump Request".to_string();
                }
                Err(e) => {
                    println!("Failed to send Dump Request: {}", e);
                    *self.status_msg.lock().unwrap() =
                        format!("Failed to send Dump Request: {}", e);
                }
            }
        } else {
            println!("Not connected to Output!");
            *self.status_msg.lock().unwrap() = "Not connected to MIDI Output".to_string();
        }
    }

    fn send_storage(&mut self) {
        if let Some(conn) = &mut self.conn_out {
            let storage = self.storage.lock().unwrap();
            let msg = storage.to_sysex();
            match conn.send(&msg) {
                Ok(_) => {
                    *self.status_msg.lock().unwrap() = format!("Sent {} bytes", msg.len());
                }
                Err(e) => {
                    println!("Failed to send Storage: {}", e);
                    *self.status_msg.lock().unwrap() = format!("Failed to send Storage: {}", e);
                }
            }
        } else {
            *self.status_msg.lock().unwrap() = "Not connected to MIDI Output".to_string();
        }
    }

    fn send_program_change(&mut self, program: u8) {
        if let Some(conn) = &mut self.conn_out {
            let msg = [0xC0, program];
            if let Err(e) = conn.send(&msg) {
                println!("Failed to send Program Change: {}", e);
            }
        }
    }

    fn send_note(&mut self, note: u8, velocity: u8, on: bool) {
        if self.audio_mode == AudioMode::Remote {
            if let Some(conn) = &mut self.conn_out {
                let cmd = if on { 0x90 } else { 0x80 };
                let msg = [cmd, note, velocity];
                if let Err(e) = conn.send(&msg) {
                    println!("Failed to send Note: {}", e);
                }
            }
        }

        if self.audio_mode == AudioMode::Local {
            if let Some(audio) = &mut self.audio {
                let storage = self.storage.lock().unwrap();
                if !storage.presets.is_empty() {
                    audio.update_preset(&storage.presets[self.current_preset_index]);
                }
                drop(storage);

                if on {
                    audio.note_on(note);
                } else {
                    audio.note_off();
                }
            }
        }
    }

    fn load_from_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PicoDSP Preset", &["pdsp"])
            .pick_file()
        {
            if let Ok(data) = fs::read(&path) {
                if let Some(new_storage) = Storage::from_sysex(&data) {
                    *self.storage.lock().unwrap() = new_storage;
                    *self.status_msg.lock().unwrap() = format!("Loaded from {}", path.display());
                    self.current_preset_index = 0;
                } else {
                    *self.status_msg.lock().unwrap() = "Failed to parse SysEx file".to_string();
                }
            } else {
                *self.status_msg.lock().unwrap() = "Failed to read file".to_string();
            }
        }
    }

    fn save_to_file(&self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PicoDSP Preset", &["pdsp"])
            .save_file()
        {
            let storage = self.storage.lock().unwrap();
            let data = storage.to_sysex();
            if fs::write(&path, data).is_ok() {
                *self.status_msg.lock().unwrap() = format!("Saved to {}", path.display());
            } else {
                *self.status_msg.lock().unwrap() = "Failed to write file".to_string();
            }
        }
    }

    fn draw_top_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(midi_in) = &self.midi_in {
                egui::ComboBox::from_id_salt("midi_in")
                    .selected_text(self.in_port_name.as_deref().unwrap_or("Select Input"))
                    .show_ui(ui, |ui| {
                        for port in midi_in.ports() {
                            let name = midi_in.port_name(&port).unwrap();
                            ui.selectable_value(&mut self.in_port_name, Some(name.clone()), name);
                        }
                    });
            }

            if let Some(midi_out) = &self.midi_out {
                egui::ComboBox::from_id_salt("midi_out")
                    .selected_text(self.out_port_name.as_deref().unwrap_or("Select Output"))
                    .show_ui(ui, |ui| {
                        for port in midi_out.ports() {
                            let name = midi_out.port_name(&port).unwrap();
                            ui.selectable_value(&mut self.out_port_name, Some(name.clone()), name);
                        }
                    });
            }

            ui.label("Audio Mode:");
            egui::ComboBox::from_id_salt("audio_mode")
                .selected_text(match self.audio_mode {
                    AudioMode::Local => "Local",
                    AudioMode::Remote => "Remote",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.audio_mode, AudioMode::Local, "Local");
                    ui.selectable_value(&mut self.audio_mode, AudioMode::Remote, "Remote");
                });

            if ui.button("Connect").clicked() {
                let out_name = self.out_port_name.clone();
                let in_name = self.in_port_name.clone().unwrap_or_default();

                if let Some(out) = out_name {
                    self.connect_midi(&in_name, &out);
                }
            }

            if ui.button("Refresh").clicked() {
                self.refresh_midi();
            }
        });

        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("Load from Device").clicked() {
                self.send_dump_request();
            }
            if ui.button("Save to Device").clicked() {
                self.send_storage();
            }

            ui.separator();

            if ui.button("Load File").clicked() {
                self.load_from_file();
            }
            if ui.button("Save File").clicked() {
                self.save_to_file();
            }

            ui.label(self.status_msg.lock().unwrap().as_str());
        });
    }
}

impl eframe::App for PicoEditApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            self.draw_top_panel(ui);
        });

        let piano_events = egui::TopBottomPanel::bottom("piano_panel")
            .min_height(150.0)
            .show(ctx, |ui| {
                ui::draw_visualizer(ui, &self.audio, &self.fft_planner);
                ui.separator();
                ui.heading("PicoDSP");
                let piano = PianoWidget::new(36, 61);
                piano.show(ui, &mut self.active_notes)
            })
            .inner;

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut storage = self.storage.lock().unwrap();
                ui::draw_preset_editor(ui, &mut storage, &mut self.current_preset_index);
            });
        });

        if self.current_preset_index != self.last_preset_index {
            self.send_program_change(self.current_preset_index as u8);
            self.last_preset_index = self.current_preset_index;

            if let Some(audio) = &mut self.audio {
                let storage = self.storage.lock().unwrap();
                if !storage.presets.is_empty() {
                    audio.update_preset(&storage.presets[self.current_preset_index]);
                }
            }
        }

        for event in piano_events {
            self.send_note(event.note, event.velocity, event.pressed);
        }
    }
}
