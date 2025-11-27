use eframe::egui;
use serialport::available_ports;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use egui_plot::{Plot, PlotPoints, Points, Text, Line};

#[derive(Default, Clone)]
struct Satellite {
    id: String,
    latitude: f64,
    longitude: f64,
    strength: u8,
}

#[derive(Default)]
struct AppState {
    ports: Vec<String>,
    selected_port: Option<String>,
    satellites: Vec<Satellite>,
    is_reading: bool,

    // 🔵 NEW: live NMEA data buffer
    nmea_log: Vec<String>,
}

pub struct MyApp {
    state: Arc<Mutex<AppState>>,
}

impl Default for MyApp {
    fn default() -> Self {
        let ports = available_ports()
            .map(|ps| ps.into_iter().map(|p| p.port_name).collect())
            .unwrap_or_default();

        Self {
            state: Arc::new(Mutex::new(AppState {
                ports,
                ..Default::default()
            })),
        }
    }
}

// =====================================================================
// Satellite Map Drawing Method
// =====================================================================
impl MyApp {
    fn draw_satellite_map(&self, ui: &mut egui::Ui, sats: &[Satellite]) {
        Plot::new("satellite_map")
            .width(300.0)
            .height(300.0)
            .view_aspect(1.0)
            .show(ui, |plot_ui| {
                // Draw outline circle
                let circle: PlotPoints = (0..360)
                    .map(|deg| {
                        let rad = (deg as f64).to_radians();
                        [rad.cos(), rad.sin()]
                    })
                    .collect::<Vec<_>>()
                    .into();

                plot_ui.line(Line::new(circle));

                // Draw satellites
                for sat in sats {
                    let az = sat.longitude.to_radians();
                    let el = sat.latitude.to_radians();

                    let x = el.cos() * az.cos();
                    let y = el.cos() * az.sin();

                    plot_ui.points(Points::new(vec![[x, y]]).radius(3.0));
                    plot_ui.text(Text::new([x, y].into(), sat.id.clone()));
                }
            });
    }
}

// =====================================================================
// Main App UI
// =====================================================================
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut state = self.state.lock().unwrap();

        // Main panel
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Select COM Port");

            let ports = state.ports.clone();

            egui::ComboBox::from_label("COM Port")
                .selected_text(state.selected_port.as_deref().unwrap_or("Select a Port"))
                .show_ui(ui, |cb| {
                    for port in ports {
                        cb.selectable_value(
                            &mut state.selected_port,
                            Some(port.clone()),
                            port,
                        );
                    }
                });

            if ui.button("Start Reading").clicked() && !state.is_reading {
                if let Some(port_name) = state.selected_port.clone() {
                    let state_clone = Arc::clone(&self.state);

                    // Thread for GPS streaming
                    thread::spawn(move || {
                        let port = serialport::new(port_name, 9600)
                            .timeout(Duration::from_millis(1000))
                            .open();

                        if let Ok(mut serial) = port {
                            let mut buf = [0u8; 1024];

                            loop {
                                match serial.read(&mut buf) {
                                    Ok(n) => {
                                        let data = String::from_utf8_lossy(&buf[..n]);
                                        let mut satellites = Vec::new();

                                        for line in data.lines() {

                                            // 🔵 Append NMEA line to log
                                            {
                                                let mut st = state_clone.lock().unwrap();
                                                st.nmea_log.push(line.to_string());

                                                // Keep log trimmed
                                                if st.nmea_log.len() > 500 {
                                                    st.nmea_log.remove(0);
                                                }
                                            }

                                            // Parse GSV
                                            if line.starts_with("$GPGSV") {
                                                let fields: Vec<&str> = line.split(',').collect();
                                                let mut i = 4;

                                                while i + 3 < fields.len() {
                                                    satellites.push(Satellite {
                                                        id: fields[i].to_string(),
                                                        latitude: fields[i + 1].parse().unwrap_or(0.0),
                                                        longitude: fields[i + 2].parse().unwrap_or(0.0),
                                                        strength: fields[i + 3].parse().unwrap_or(0),
                                                    });
                                                    i += 4;
                                                }
                                            }
                                        }

                                        // Update satellites
                                        let mut st = state_clone.lock().unwrap();
                                        st.satellites = satellites;
                                    }
                                    Err(_) => break,
                                }

                                thread::sleep(Duration::from_millis(200));
                            }
                        }
                    });

                    state.is_reading = true;
                }
            }

            ui.separator();
            ui.heading("Satellites");

            egui::ScrollArea::vertical().show(ui, |ui| {
                for sat in &state.satellites {
                    ui.horizontal(|ui| {
                        ui.label(format!("ID: {}", sat.id));
                        ui.label(format!("Elv: {:.2}", sat.latitude));
                        ui.label(format!("Azm: {:.2}", sat.longitude));
                        ui.label(format!("Strength: {}", sat.strength));
                    });
                }
            });
        });

        // =====================================================================
        // NEW: Live GPS Stream Window
        // =====================================================================
        egui::Window::new("GPS Stream")
            .default_width(400.0)
            .default_height(300.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("Live NMEA Data:");

                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &state.nmea_log {
                            ui.monospace(line);
                        }
                    });
            });

        // =====================================================================
        // Mini floating sky map
        // =====================================================================
        egui::Area::new("mini_sky_map".into())
            .anchor(egui::Align2::RIGHT_TOP, [-10.0, 10.0])
            .show(ctx, |ui| {
                self.draw_satellite_map(ui, &state.satellites);
            });
    }
}

// =====================================================================
// Run
// =====================================================================
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "NMEA GPS Viewer",
        options,
        Box::new(|_cc| Box::new(MyApp::default())),
    )
}
