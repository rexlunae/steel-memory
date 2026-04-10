use std::{env, process};

use steel_memory_lib::benchmark;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    benchmark::run_cli(env::args().skip(1).collect())
}
