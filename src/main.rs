use rusocks::cli::CLI;
use std::process;

fn main() {
    let cli = CLI::new();

    if let Err(err) = cli.execute() {
        eprintln!("{}", err);
        process::exit(1);
    }
}
