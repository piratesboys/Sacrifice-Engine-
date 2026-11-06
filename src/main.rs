mod attack;
mod board;
mod center;
mod evaluate;
mod pgn;
mod sacrifice;
mod search;
mod uci;

use crate::pgn::StyleBook;
use std::path::PathBuf;

fn main() {

    println!("Sacrifice-Engine");

    let mut style_book = StyleBook::new();

    let pgn_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("Tal.pgn");

    println!("Loading Tal's playing style...");
    println!("PGN file: {}", pgn_path.display());

    match style_book.load_file(pgn_path.to_str().unwrap_or("data/Tal.pgn")) {
        Ok(()) => {
            println!("Tal's playing style loaded successfully!");
            println!("  Games      : {}", style_book.games());
            println!("  Positions  : {}", style_book.positions());
            println!("  Moves      : {}", style_book.moves());
            println!("  Sacrifices : {}", style_book.sacrifices());
            println!("  Checks     : {}", style_book.checks());
            println!("  Captures   : {}", style_book.captures());
        }
        Err(error) => {
            eprintln!("ERROR: Failed to load Tal.pgn");
            eprintln!("{}", error);
            println!("The engine will start with an empty StyleBook.");
        }
    }

    println!();

    uci::run_uci_with_style(style_book);
}
