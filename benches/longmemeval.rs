use std::{env, process};

fn main() {
    if let Err(error) = steel_memory_lib::benchmark::run_cli(env::args().skip(1).collect()) {
        eprintln!("{error:#}");
        process::exit(1);
    }
}
