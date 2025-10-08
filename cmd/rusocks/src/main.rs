use rusocks::cli::CLI;
use std::process;

fn main() {
    // Create a new CLI instance
    let cli = CLI::new();

    // Execute the CLI and handle errors
    if let Err(err) = cli.execute() {
        eprintln!("{}", err);
        process::exit(1);
    }
}