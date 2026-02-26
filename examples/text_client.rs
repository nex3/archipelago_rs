use std::{collections::VecDeque, mem};

use archipelago_rs as ap;
use eframe::{Storage, egui::*};
use serde::{Deserialize, Serialize};
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};

/// The maximum number of prints to store at once.
const PRINT_BUFFER: usize = 0x2000;

fn main() -> Result<(), anyhow::Error> {
    TermLogger::init(
        LevelFilter::Info,
        Default::default(),
        TerminalMode::Stderr,
        ColorChoice::Auto,
    )?;

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default().with_inner_size([600.0, 240.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Archipelago Example",
        options,
        Box::new(|ctx| {
            Ok(Box::new(ArchipelagoClient {
                connect_popup: ctx
                    .storage
                    .and_then(|s| s.get_string("connect_popup"))
                    .as_deref()
                    .map(serde_json::from_str)
                    .map(Result::unwrap_or_default)
                    .unwrap_or_default(),
                ..Default::default()
            }))
        }),
    )
    .unwrap();

    Ok(())
}

#[derive(Default)]
struct ArchipelagoClient {
    connection: ap::Connection<()>,
    connect_popup: ConnectPopup,
    prints: VecDeque<ap::Print>,
    message: String,
}

impl eframe::App for ArchipelagoClient {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        // Force the app to continually paint new frames so that we don't starve
        // the Archipelago connection. If you're running as part of a game's UI
        // loop, you don't need to worry about this, since the game will render
        // many frames per second anyway.
        ctx.request_repaint();

        for event in self.connection.update() {
            if let ap::Event::Print(print) = event {
                if self.prints.len() >= PRINT_BUFFER {
                    self.prints.pop_front();
                }
                self.prints.push_back(print);
            }
        }

        CentralPanel::default().show(ctx, |ui| {
            match self.connection.state() {
                ap::ConnectionState::Connecting(_) => {
                    ui.heading("Connecting...");
                }
                ap::ConnectionState::Connected(_) => {
                    ui.label(RichText::new("Connected").heading().color(Color32::GREEN));
                }
                ap::ConnectionState::Disconnected(err) => {
                    ui.label(RichText::new("Disconnected").heading().color(Color32::RED));
                    ui.label(format!("{}", err));
                }
            }
            ui.separator();

            match self.connection.state_mut() {
                ap::ConnectionState::Disconnected(_) => {
                    if ui.button("Connect").clicked() {
                        self.connect_popup.visible = true;
                    }
                }
                ap::ConnectionState::Connected(client) => {
                    ScrollArea::vertical()
                        .max_height(ui.available_height() - 30.)
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for print in &self.prints {
                                ui.label(print.to_string());
                            }
                        });
                    ui.separator();

                    let response = ui
                        .add(TextEdit::singleline(&mut self.message).desired_width(f32::INFINITY));
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let _ = client.say(mem::take(&mut self.message));
                    }
                }
                _ => {}
            }
        });

        if self.connect_popup.visible {
            let response = self.connect_popup.update(ctx, frame);
            if let Some(connection) = response.inner {
                self.connect_popup.visible = false;
                self.connection = connection;
            } else if response.should_close() {
                self.connect_popup.visible = false;
            }
        }
    }

    fn save(&mut self, storage: &mut dyn Storage) {
        storage.set_string(
            "connect_popup",
            serde_json::to_string(&self.connect_popup).unwrap_or_default(),
        )
    }
}

#[derive(Default, Serialize, Deserialize)]
struct ConnectPopup {
    url: String,
    slot: String,

    #[serde(skip)]
    visible: bool,
}

impl ConnectPopup {
    fn update(
        &mut self,
        ctx: &Context,
        _: &mut eframe::Frame,
    ) -> ModalResponse<Option<ap::Connection<()>>> {
        Modal::new(Id::new("connect-popup")).show(ctx, |ui| {
            let responses = [
                ui.horizontal(|ui| {
                    ui.label("URL");
                    ui.add(TextEdit::singleline(&mut self.url).hint_text("archipelago.gg:12345"))
                }),
                ui.horizontal(|ui| {
                    ui.label("Slot");
                    ui.add(TextEdit::singleline(&mut self.slot))
                }),
            ];

            if ui.button("Connect").clicked()
                || (responses.into_iter().any(|r| r.inner.lost_focus())
                    && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                Some(ap::Connection::new(
                    &self.url,
                    &self.slot,
                    None::<String>,
                    ap::ConnectionOptions::new().tags(vec!["TextOnly"]),
                ))
            } else {
                None
            }
        })
    }
}
