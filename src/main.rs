use std::io;
use std::io::stdout;

use clap::Parser;
use crossterm::{
    cursor::{DisableBlinking, MoveTo},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};

mod renderloop;
mod search;
mod utils;

#[derive(Parser)]
#[clap(
    version = env!("CARGO_PKG_VERSION"),
    author = env!("CARGO_PKG_AUTHORS")
)]
struct Opts {
    input: String,
}

fn main() -> io::Result<()> {
    let opts: Opts = Opts::parse();
    let mut stdout = stdout();

    enable_raw_mode()?;

    execute!(stdout, Clear(ClearType::All))?;

    execute!(stdout, MoveTo(0, 0), DisableBlinking)?;

    if let Err(e) = renderloop::less_loop(opts.input.as_str()) {
        println!("error={:?}\r", e);
    }

    disable_raw_mode()
}
