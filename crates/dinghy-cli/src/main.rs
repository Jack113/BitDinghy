//! The `dinghy` command-line program. A thin front-end: it parses what you typed and hands the
//! real work to `dinghy-core`. Commands such as `add`, `list` and `pause` arrive feature by feature.

use clap::Parser;

/// BitDinghy on the command line.
#[derive(Parser)]
#[command(name = "dinghy", version, about)]
struct Cli {}

fn main() {
    // `parse()` handles `--help` and `--version` for us and exits; everything else lands here.
    let _cli = Cli::parse();

    println!("{}", dinghy_core::about());
    println!("No commands yet: they arrive one feature at a time. Try `dinghy --help`.");
}
