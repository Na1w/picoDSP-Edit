use crate::audio::AudioManager;
use crate::protocol::{Preset, Storage, Waveform};
use eframe::egui;
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::{Arc, Mutex};

pub fn draw_visualizer(
    ui: &mut egui::Ui,
    audio: &Option<AudioManager>,
    fft_planner: &Arc<Mutex<FftPlanner<f32>>>,
) {
    if let Some(audio) = audio {
        let height = 120.0;
        let available_width = ui.available_width();

        ui.horizontal(|ui| {
            // Left: Oscilloscope
            let (response, painter) = ui.allocate_painter(
                egui::Vec2::new(available_width * 0.5, height),
                egui::Sense::hover(),
            );

            let rect = response.rect;
            painter.rect_filled(rect, 2.0, egui::Color32::from_rgb(20, 20, 20));
            painter.rect_stroke(rect, 1.0, egui::Stroke::new(1.0, egui::Color32::DARK_GRAY));

            if let Ok(buffer) = audio.scope_buffer.try_lock() {
                // Oscilloscope
                let points: Vec<egui::Pos2> = buffer
                    .iter()
                    .enumerate()
                    .map(|(i, &sample)| {
                        let x = rect.min.x + (i as f32 / buffer.len() as f32) * rect.width();
                        let y = rect.center().y - sample * (height * 0.9);
                        egui::pos2(x, y)
                    })
                    .collect();
                painter.add(egui::Shape::line(
                    points,
                    egui::Stroke::new(1.5, egui::Color32::GREEN),
                ));

                // Right: Spectrum (FFT)
                let (response_fft, painter_fft) = ui.allocate_painter(
                    egui::Vec2::new(available_width * 0.5 - 5.0, height), // -5 for spacing
                    egui::Sense::hover(),
                );
                let rect_fft = response_fft.rect;
                painter_fft.rect_filled(rect_fft, 2.0, egui::Color32::from_rgb(20, 20, 20));
                painter_fft.rect_stroke(
                    rect_fft,
                    1.0,
                    egui::Stroke::new(1.0, egui::Color32::DARK_GRAY),
                );

                // Compute FFT
                let mut planner = fft_planner.lock().unwrap();
                let fft = planner.plan_fft_forward(buffer.len());

                let mut input: Vec<Complex<f32>> =
                    buffer.iter().map(|&s| Complex::new(s, 0.0)).collect();
                fft.process(&mut input);

                // Draw Spectrum (Magnitude)
                // Only display first half (Nyquist)
                let spectrum_len = input.len() / 2;
                let bar_width = rect_fft.width() / spectrum_len as f32;

                for (i, complex) in input.iter().take(spectrum_len).enumerate() {
                    let magnitude = complex.norm();
                    // Logarithmic scaling for better visualization
                    let scaled_mag = (magnitude / 10.0).clamp(0.0, 1.0);

                    let x = rect_fft.min.x + i as f32 * bar_width;
                    let bar_height = scaled_mag * rect_fft.height();
                    let y = rect_fft.max.y - bar_height;

                    painter_fft.rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(x, y),
                            egui::pos2(x + bar_width, rect_fft.max.y),
                        ),
                        0.0,
                        egui::Color32::from_rgb(100, 150, 255).linear_multiply(0.8),
                    );
                }
            }
        });

        ui.ctx().request_repaint();
    }
}

pub fn draw_preset_editor(
    ui: &mut egui::Ui,
    storage: &mut Storage,
    current_preset_index: &mut usize,
) {
    if storage.presets.is_empty() {
        ui.label("No presets loaded.");
        return;
    }

    ui.horizontal(|ui| {
        ui.label("Preset:");

        if ui.button("<").clicked() && *current_preset_index > 0 {
            *current_preset_index -= 1;
        }

        egui::ComboBox::from_id_salt("preset_selector")
            .selected_text(format!(
                "{}: {}",
                *current_preset_index + 1,
                storage.presets[*current_preset_index].name
            ))
            .show_ui(ui, |ui| {
                for (i, preset) in storage.presets.iter().enumerate() {
                    ui.selectable_value(
                        current_preset_index,
                        i,
                        format!("{}: {}", i + 1, preset.name),
                    );
                }
            });

        if ui.button(">").clicked() && *current_preset_index < storage.presets.len() - 1 {
            *current_preset_index += 1;
        }

        if ui.button("Add New").clicked() {
            storage.presets.push(Preset::default());
            *current_preset_index = storage.presets.len() - 1;
        }

        if ui.button("Clone").clicked() {
            let mut new_preset = storage.presets[*current_preset_index].clone();
            new_preset.name = format!("{} Copy", new_preset.name);
            if new_preset.name.len() > 32 {
                new_preset.name.truncate(32);
            }
            storage.presets.push(new_preset);
            *current_preset_index = storage.presets.len() - 1;
        }
    });

    ui.separator();

    let preset = &mut storage.presets[*current_preset_index];

    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.text_edit_singleline(&mut preset.name);
    });

    ui.columns(3, |cols| {
        for (i, col) in cols.iter_mut().enumerate() {
            col.heading(format!("Oscillator {}", i + 1));
            let osc = match i {
                0 => &mut preset.osc1,
                1 => &mut preset.osc2,
                _ => &mut preset.osc3,
            };

            col.horizontal(|ui| {
                ui.label("Wave:");
                egui::ComboBox::from_id_salt(format!("wave_{}", i))
                    .selected_text(format!("{:?}", osc.waveform))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut osc.waveform, Waveform::Sine, "Sine");
                        ui.selectable_value(&mut osc.waveform, Waveform::Triangle, "Triangle");
                        ui.selectable_value(&mut osc.waveform, Waveform::Saw, "Saw");
                        ui.selectable_value(&mut osc.waveform, Waveform::Square, "Square");
                        ui.selectable_value(&mut osc.waveform, Waveform::Noise, "Noise");
                    });
            });

            col.add(egui::Slider::new(&mut osc.level, 0.0..=1.0).text("Level"));
            col.add(egui::Slider::new(&mut osc.octave, -2.0..=2.0).text("Octave"));
            col.add(egui::Slider::new(&mut osc.detune, -100.0..=100.0).text("Detune"));
            col.checkbox(&mut osc.vibrato, "Vibrato");
        }
    });

    ui.separator();

    ui.columns(3, |cols| {
        cols[0].heading("Filter");
        cols[0].add(
            egui::Slider::new(&mut preset.filter.cutoff, 20.0..=20000.0)
                .text("Cutoff")
                .logarithmic(true),
        );
        cols[0].add(egui::Slider::new(&mut preset.filter.resonance, 0.0..=1.0).text("Resonance"));
        cols[0]
            .add(egui::Slider::new(&mut preset.filter.env_amt, -10000.0..=10000.0).text("Env Amt"));

        cols[0].label("Filter Envelope");
        cols[0].horizontal(|ui| {
            ui.add(
                egui::Slider::new(&mut preset.filter.attack, 0.0..=5.0)
                    .text("A")
                    .vertical(),
            );
            ui.add(
                egui::Slider::new(&mut preset.filter.decay, 0.0..=5.0)
                    .text("D")
                    .vertical(),
            );
            ui.add(
                egui::Slider::new(&mut preset.filter.sustain, 0.0..=1.0)
                    .text("S")
                    .vertical(),
            );
            ui.add(
                egui::Slider::new(&mut preset.filter.release, 0.0..=5.0)
                    .text("R")
                    .vertical(),
            );
        });

        cols[1].heading("Amp Envelope");
        cols[1].horizontal(|ui| {
            ui.add(
                egui::Slider::new(&mut preset.amp.attack, 0.0..=5.0)
                    .text("A")
                    .vertical(),
            );
            ui.add(
                egui::Slider::new(&mut preset.amp.decay, 0.0..=5.0)
                    .text("D")
                    .vertical(),
            );
            ui.add(
                egui::Slider::new(&mut preset.amp.sustain, 0.0..=1.0)
                    .text("S")
                    .vertical(),
            );
            ui.add(
                egui::Slider::new(&mut preset.amp.release, 0.0..=5.0)
                    .text("R")
                    .vertical(),
            );
        });

        cols[1].add(egui::Slider::new(&mut preset.noise, 0.0..=1.0).text("Noise Level"));
        cols[1].add(egui::Slider::new(&mut preset.portamento, 0.0..=1.0).text("Portamento"));

        cols[2].heading("Effects");
        cols[2].label("Delay");
        cols[2].checkbox(&mut preset.delay.enabled, "Enable Delay");
        cols[2].add(egui::Slider::new(&mut preset.delay.time, 0.0..=2.0).text("Time"));
        cols[2].add(egui::Slider::new(&mut preset.delay.feedback, 0.0..=1.0).text("Feedback"));
        cols[2].add(egui::Slider::new(&mut preset.delay.mix, 0.0..=1.0).text("Mix"));

        cols[2].separator();
        cols[2].label("Reverb");
        cols[2].checkbox(&mut preset.reverb.enabled, "Enable Reverb");
        cols[2].add(egui::Slider::new(&mut preset.reverb.size, 0.0..=1.0).text("Size"));
        cols[2].add(egui::Slider::new(&mut preset.reverb.damping, 0.0..=1.0).text("Damping"));
        cols[2].add(egui::Slider::new(&mut preset.reverb.mix, 0.0..=1.0).text("Mix"));
    });
}
