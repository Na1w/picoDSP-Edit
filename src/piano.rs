use eframe::egui;

pub struct PianoWidget {
    start_note: u8,
    key_count: u8,
}

impl Default for PianoWidget {
    fn default() -> Self {
        Self {
            start_note: 48, // C3
            key_count: 24,  // 2 octaves
        }
    }
}

pub struct PianoEvent {
    pub note: u8,
    pub velocity: u8,
    pub pressed: bool,
}

impl PianoWidget {
    pub fn new(start_note: u8, key_count: u8) -> Self {
        Self {
            start_note,
            key_count,
        }
    }

    pub fn show(&self, ui: &mut egui::Ui, active_notes: &mut Vec<u8>) -> Vec<PianoEvent> {
        let mut events = Vec::new();

        let height = 100.0;

        // Calculate number of white keys
        let white_keys_count = (0..self.key_count)
            .filter(|&i| !is_black_key(self.start_note + i))
            .count();

        // Dynamic width based on available space
        let available_width = ui.available_width();
        let white_key_width = available_width / white_keys_count as f32;
        let black_key_width = white_key_width * 0.65; // Standard ratio
        let black_key_height = height * 0.6;

        let (response, painter) = ui.allocate_painter(
            egui::Vec2::new(available_width, height),
            egui::Sense::click_and_drag(),
        );

        let mouse_pos = response.hover_pos();
        let mouse_down = response.is_pointer_button_down_on();
        let widget_rect = response.rect;

        // Helper to get key rect
        let get_key_rect = |is_black: bool, white_key_idx: usize| -> egui::Rect {
            let x = widget_rect.min.x + (white_key_idx as f32 * white_key_width);
            if is_black {
                egui::Rect::from_min_size(
                    egui::pos2(x - (black_key_width / 2.0), widget_rect.min.y),
                    egui::vec2(black_key_width, black_key_height),
                )
            } else {
                egui::Rect::from_min_size(
                    egui::pos2(x, widget_rect.min.y),
                    egui::vec2(white_key_width, height),
                )
            }
        };

        // Draw Keys
        let mut white_key_idx = 0;
        for i in 0..self.key_count {
            let note = self.start_note + i;
            let is_black = is_black_key(note);

            if !is_black {
                let rect = get_key_rect(false, white_key_idx);
                let is_pressed = active_notes.contains(&note);
                let fill_color = if is_pressed {
                    egui::Color32::from_rgb(200, 200, 255)
                } else {
                    egui::Color32::WHITE
                };
                painter.rect_filled(rect, 2.0, fill_color);
                painter.rect_stroke(rect, 1.0, egui::Stroke::new(1.0, egui::Color32::BLACK));
                white_key_idx += 1;
            }
        }

        let mut white_key_idx = 0;
        for i in 0..self.key_count {
            let note = self.start_note + i;
            let is_black = is_black_key(note);

            if is_black {
                let rect = get_key_rect(true, white_key_idx);
                let is_pressed = active_notes.contains(&note);
                let fill_color = if is_pressed {
                    egui::Color32::from_rgb(100, 100, 200)
                } else {
                    egui::Color32::BLACK
                };
                painter.rect_filled(rect, 2.0, fill_color);
                painter.rect_stroke(rect, 1.0, egui::Stroke::new(1.0, egui::Color32::DARK_GRAY));
            } else {
                white_key_idx += 1;
            }
        }

        // Hit Testing
        if let Some(pos) = mouse_pos {
            if widget_rect.contains(pos) {
                let mut hit_note = None;

                // Check Black Keys first
                let mut white_key_idx = 0;
                for i in 0..self.key_count {
                    let note = self.start_note + i;
                    if is_black_key(note) {
                        let rect = get_key_rect(true, white_key_idx);
                        if rect.contains(pos) {
                            hit_note = Some(note);
                            break;
                        }
                    } else {
                        white_key_idx += 1;
                    }
                }

                // Check White Keys
                if hit_note.is_none() {
                    let mut white_key_idx = 0;
                    for i in 0..self.key_count {
                        let note = self.start_note + i;
                        if !is_black_key(note) {
                            let rect = get_key_rect(false, white_key_idx);
                            if rect.contains(pos) {
                                hit_note = Some(note);
                                break;
                            }
                            white_key_idx += 1;
                        }
                    }
                }

                if mouse_down {
                    if let Some(note) = hit_note {
                        if !active_notes.contains(&note) {
                            // Monophonic mouse interaction for simplicity
                            for old_note in active_notes.iter() {
                                if *old_note != note {
                                    events.push(PianoEvent {
                                        note: *old_note,
                                        velocity: 0,
                                        pressed: false,
                                    });
                                }
                            }
                            active_notes.clear();

                            active_notes.push(note);
                            events.push(PianoEvent {
                                note,
                                velocity: 100,
                                pressed: true,
                            });
                        }
                    }
                } else {
                    for note in active_notes.drain(..) {
                        events.push(PianoEvent {
                            note,
                            velocity: 0,
                            pressed: false,
                        });
                    }
                }
            } else {
                for note in active_notes.drain(..) {
                    events.push(PianoEvent {
                        note,
                        velocity: 0,
                        pressed: false,
                    });
                }
            }
        } else if !mouse_down {
            for note in active_notes.drain(..) {
                events.push(PianoEvent {
                    note,
                    velocity: 0,
                    pressed: false,
                });
            }
        }

        events
    }
}

fn is_black_key(note: u8) -> bool {
    match note % 12 {
        1 | 3 | 6 | 8 | 10 => true,
        _ => false,
    }
}
