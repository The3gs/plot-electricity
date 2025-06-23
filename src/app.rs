use anyhow::{anyhow, bail, ensure, Context};

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    error: Option<String>,

    data: Option<UsageData>,

    year: u16,
    month: u8,
    day: u8,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct UsageEntry {
    date: (u16, u8, u8),
    interval_start_hour: u8,
    interval_end_hour: u8,
    kilowatt_hours: f64,
    note: String,
}

impl UsageEntry {
    fn parse(line: &str) -> anyhow::Result<Self> {
        let mut iterator = line.split(',');

        let entry_type = iterator.next();
        if entry_type != Some("Electric usage") {
            if let Some(value) = entry_type {
                bail!("Unknown entry type {value:?}")
            } else {
                bail!("Insuffiecient entries in line, ")
            }
        }

        let date: (u16, u8, u8);
        if let Some(date_str) = iterator.next() {
            let mut date_parts = date_str.split('/');

            let month = date_parts
                .next()
                .ok_or_else(|| anyhow!("Expected Month"))?
                .parse()?;
            let day = date_parts
                .next()
                .ok_or_else(|| anyhow!("Expected Day"))?
                .parse()?;
            let year = date_parts
                .next()
                .ok_or_else(|| anyhow!("Expected Year"))?
                .parse()?;

            date = (year, month, day);
        } else {
            bail!("Insufficient entries in line, expected date.")
        }

        let interval_start_hour;
        if let Some(interval_start_str) = iterator.next() {
            let Some((hour, rest)) = interval_start_str.split_once(':') else {
                bail!("Invalid date {interval_start_str:?}")
            };

            let Some((minutes, hemiday)) = rest.split_once(' ') else {
                bail!("Invalid date {interval_start_str:?}")
            };

            ensure!(
                minutes == "00",
                "Program is illequiped to handle non-zero minutes"
            );

            let hour = hour.parse::<u8>()? % 12;
            interval_start_hour = hour
                + match hemiday {
                    "AM" | "am" => 0,
                    "PM" | "pm" => 12,
                    _ => bail!("Unknown hemiday {hemiday:?}, AM/PM or am/pm accepted"),
                }
        } else {
            bail!("Insuffiecient entries in line, expected start time");
        }

        let interval_end_hour;
        if let Some(interval_end_str) = iterator.next() {
            let Some((hour, rest)) = interval_end_str.split_once(':') else {
                bail!("Invalid date {interval_end_str:?}")
            };

            let Some((minutes, hemiday)) = rest.split_once(' ') else {
                bail!("Invalid date {interval_end_str:?}")
            };

            ensure!(
                minutes == "00",
                "Program is illequiped to handle non-zero minutes"
            );

            let hour = hour.parse::<u8>()? % 12;
            interval_end_hour = hour
                + match hemiday {
                    "AM" | "am" => 0,
                    "PM" | "pm" => 12,
                    _ => bail!("Unknown hemiday {hemiday:?}, AM/PM or am/pm accepted"),
                }
        } else {
            bail!("Insuffiecient entries in line, expected end time");
        }

        let kilowatt_hours;
        if let (Some(scalar), Some(unit)) = (iterator.next(), iterator.next()) {
            let scalar = scalar.parse::<f64>().unwrap();
            kilowatt_hours = scalar
                * match unit {
                    "kWh" => 1.0,
                    _ => bail!("Unknown unit {unit:?}"),
                };
        } else {
            bail!("Insufficient entries in line, expected usage and unit");
        }

        let Some(note) = iterator.next().map(|v| v.to_owned()) else {
            bail!("Expected final entry for note (may be empty, but comma is expected)")
        };

        ensure!(iterator.next().is_none(), "Extra entries in the line");

        Ok(Self {
            date,
            interval_start_hour,
            interval_end_hour,
            kilowatt_hours,
            note,
        })
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct UsageData {
    address: String,
    entries: Vec<UsageEntry>,
}

impl UsageData {
    fn parse(input: &str) -> anyhow::Result<Self> {
        let mut lines = input.split("\n");

        let Some(address) = lines.next().map(|s| s.to_string()) else {
            bail!("Expected an address at the top of the file")
        };

        ensure!(lines.next() == Some(""), "Expected second line to be blank");
        ensure!(
            lines.next() == Some("TYPE,DATE,START TIME,END TIME,USAGE,UNITS,NOTES"),
            "Incorrect headers. Format must have changed or something. Sorry"
        );

        let entries = lines
            .filter(|s| !s.is_empty())
            .enumerate()
            .map(|(i, s)| UsageEntry::parse(s).with_context(|| format!("On line {}", i + 3)))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { address, entries })
    }
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            error: None,
            data: None,
            year: 2025,
            month: 06,
            day: 21,
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Default::default()
    }
}

impl eframe::App for TemplateApp {
    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

            egui::menu::bar(ui, |ui| {
                // NOTE: no File->Quit on web pages!
                ui.menu_button("File", |ui| {
                    if ui.button("Open").clicked() {
                        if let Some(file) = rfd::FileDialog::new().pick_file() {
                            if let Ok(file) = std::fs::read_to_string(file) {
                                match UsageData::parse(&file) {
                                    Ok(data) => {
                                        if let Some(last) = data.entries.last() {
                                            self.year = last.date.0;
                                            self.month = last.date.1;
                                            self.day = last.date.2;
                                        }
                                        self.data = Some(data);
                                    }
                                    Err(error) => self.error = Some(error.to_string()),
                                }
                            }
                        }
                    }
                    let is_web = cfg!(target_arch = "wasm32");
                    if !is_web {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                });
                ui.menu_button("About", |ui| {
                    ui.hyperlink_to("Source code.", "https://github.com/The3gs/plot-electricity");

                    powered_by_egui_and_eframe(ui);
                    egui::warn_if_debug_build(ui);
                });
                ui.add_space(16.0);

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            // The central panel the region left after adding TopPanel's and SidePanel's
            ui.heading("Power usage display");

            if let Some(e) = &self.error {
                ui.label(e);
                if ui.button("Clear Error").clicked() {
                    self.error = None;
                }
            }

            ui.separator();

            if let Some(data) = &self.data {
                ui.label(&data.address);
                let mut date = chrono::NaiveDate::from_ymd_opt(
                    self.year as i32,
                    self.month as u32,
                    self.day as u32,
                )
                .unwrap();
                ui.add(egui_extras::DatePickerButton::new(&mut date));
                {
                    use chrono::Datelike;
                    self.year = date.year() as u16;
                    self.month = date.month() as u8;
                    self.day = date.day() as u8;
                }

                let plot = egui_plot::Plot::new("Power Usage Chart")
                    .legend(egui_plot::Legend::default())
                    .show_axes(true)
                    .show_grid(false);
                plot.show(ui, |plot_ui| {
                    plot_ui.bar_chart(egui_plot::BarChart::new(
                        format!("Usage for {}", date.format("%y-%m-%d").to_string()),
                        data.entries
                            .iter()
                            .filter(|data_entry| {
                                data_entry.date == (self.year, self.month, self.day)
                            })
                            .map(|data_entry| {
                                egui_plot::Bar::new(
                                    data_entry.interval_start_hour as f64,
                                    data_entry.kilowatt_hours,
                                )
                                .width(1.0)
                            })
                            .collect::<Vec<_>>(),
                    ))
                });
            }
        });
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}
