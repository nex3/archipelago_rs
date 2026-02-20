use std::mem;

use archipelago_rs as ap;
use eframe::egui::*;
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};

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
        Box::new(|_| Ok(Box::<ArchipelagoClient>::default())),
    )
    .unwrap();

    Ok(())
}

#[derive(Default)]
struct ArchipelagoClient {
    connection: ap::Connection<()>,
    connect_popup: Option<ConnectPopup>,
    prints: Vec<ap::Print>,
    message: String,
}

impl eframe::App for ArchipelagoClient {
    fn update(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        for event in self.connection.update() {
            if let ap::Event::Print(print) = event {
                self.prints.push(print);
            }
        }

        CentralPanel::default().show(ctx, |ui| {
            match self.connection.state() {
                ap::ConnectionState::Connecting(_) => {
                    ui.heading("Connecting...");
                }
                ap::ConnectionState::Connected(client) => {
                    ui.label(RichText::new("Connected").heading().color(Color32::GREEN));
                    ui.label(format!("Slot data: {:?}", client.slot_data()));
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
                        self.connect_popup = Some(Default::default());
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

        if let Some(popup) = &mut self.connect_popup {
            let response = popup.update(ctx, frame);
            if let Some(connection) = response.inner {
                self.connect_popup = None;
                self.connection = connection;
            } else if response.should_close() {
                self.connect_popup = None;
            }
        }
    }
}

#[derive(Default)]
struct ConnectPopup {
    url: String,
    game: String,
    slot: String,
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
                    ui.label("Game");
                    ui.add(TextEdit::singleline(&mut self.game).hint_text("Dark Souls III"))
                }),
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
                    &self.game,
                    &self.slot,
                    Default::default(),
                ))
            } else {
                None
            }
        })
    }
}
