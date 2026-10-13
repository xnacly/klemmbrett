mod cli;

fn main() {
    let args = <cli::Cli as clap::Parser>::parse();
    if args.verbose {
        println!("{:?}", args);
    }
}
