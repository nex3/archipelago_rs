use std::io::{self, BufRead};
use std::time::Duration;

use archipelago_rs::{Connection, ConnectionStateType};
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};

fn main() -> anyhow::Result<()> {
    TermLogger::init(
        LevelFilter::Debug,
        Default::default(),
        TerminalMode::Stderr,
        ColorChoice::Auto,
    )?;

    // Connect to AP server
    let server = prompt("Connect to what AP server?")?;
    let game = prompt("What game?")?;
    let slot = prompt("What slot?")?;

    let mut connection: Connection = Connection::new(server, game, slot, Default::default());
    loop {
        if let Some(transition) = connection.update() {
            println!("{:?}", transition);
        }
        if connection.state_type() == ConnectionStateType::Disconnected {
            return Err(connection.into_err().into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn prompt(text: &str) -> Result<String, anyhow::Error> {
    println!("{}", text);

    Ok(io::stdin().lock().lines().next().unwrap()?)
}
