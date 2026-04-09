use std::{env, path::PathBuf, process};

use steel_memory_lib::benchmark::{
    LongMemEvalBenchmark, LongMemEvalBenchmarkOptions, LongMemEvalGranularity,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let options = parse_args(env::args().skip(1).collect())?;
    let mut benchmark = LongMemEvalBenchmark::new()?;
    let run = benchmark.run_path(&options.data_path, &options.benchmark_options)?;
    println!("{}", serde_json::to_string_pretty(&run.summary)?);
    Ok(())
}

struct CliOptions {
    data_path: PathBuf,
    benchmark_options: LongMemEvalBenchmarkOptions,
}

fn parse_args(args: Vec<String>) -> anyhow::Result<CliOptions> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage();
        process::exit(0);
    }

    let mut data_path = None;
    let mut granularity = LongMemEvalGranularity::Session;
    let mut max_questions = None;
    let mut output_path = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data" => data_path = Some(PathBuf::from(next_value(&mut args, "--data")?)),
            "--granularity" => {
                let value = next_value(&mut args, "--granularity")?;
                granularity = value.parse()?;
            }
            "--max-questions" => {
                let value = next_value(&mut args, "--max-questions")?;
                max_questions = Some(value.parse()?);
            }
            "--output" => output_path = Some(PathBuf::from(next_value(&mut args, "--output")?)),
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }

    let data_path = data_path.ok_or_else(|| anyhow::anyhow!("--data is required"))?;
    Ok(CliOptions {
        data_path,
        benchmark_options: LongMemEvalBenchmarkOptions {
            granularity,
            max_questions,
            output_path,
        },
    })
}

fn next_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> anyhow::Result<String> {
    args.next()
        .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))
}

fn print_usage() {
    eprintln!(
        "Usage: cargo run --bin longmemeval-benchmark -- --data <path> [--granularity session|turn] [--max-questions N] [--output <jsonl-path>]"
    );
}
