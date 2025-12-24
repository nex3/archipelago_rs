use archipelago_rs::{Connection, ConnectionState};
use eframe::egui::{self, Color32, RichText};
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};

fn main() -> Result<(), anyhow::Error> {
    TermLogger::init(
        LevelFilter::Info,
        Default::default(),
        TerminalMode::Stderr,
        ColorChoice::Auto,
    )?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
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
    connection: Connection,
    url: String,
    game: String,
    slot: String,
}

impl eframe::App for ArchipelagoClient {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.connection.update();
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.connection.state() {
                ConnectionState::Connecting(_) => {
                    ui.heading("Connecting...");
                }
                ConnectionState::Connected(_) => {
                    ui.label(RichText::new("Connected").heading().color(Color32::GREEN));
                }
                ConnectionState::Disconnected(err) => {
                    ui.label(RichText::new("Disconnected").heading().color(Color32::RED));
                    ui.label(format!("{}", err));
                }
            }
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Game");
                ui.add(egui::TextEdit::singleline(&mut self.game).hint_text("Dark Souls III"));
            });
            ui.horizontal(|ui| {
                ui.label("URL");
                ui.add(
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text("wss://archipelago.gg:12345"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Slot");
                ui.add(egui::TextEdit::singleline(&mut self.slot));
            });
            if ui.button("Connect").clicked() {
                self.connection =
                    Connection::new(&self.url, &self.game, &self.slot, Default::default());
            }
        });
    }
}

//     // Connect to AP server
//     // let server = prompt("Connect to what AP server?")?;
//     // let game = prompt("What game?")?;-
//     // let slot = prompt("What slot?")?;

//     let executor = LocalExecutor::new();

//     let mut connection: Connection = Connection::new(
//         "wss://archipelago.gg:42447",
//         "Dark Souls III",
//         "Natalie",
//         Default::default(),
//     );
//     loop {
//         if let Some(transition) = connection.update() {
//             println!("{:?}", transition);
//         }
//         if connection.state_type() == ConnectionStateType::Disconnected {
//             return Err(connection.into_err().into());
//         }
//         std::thread::sleep(Duration::from_millis(10));
//     }
// }

// fn prompt(text: &str) -> Result<String, anyhow::Error> {
//     println!("{}", text);

//     Ok(io::stdin().lock().lines().next().unwrap()?)
// }
