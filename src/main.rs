mod cli;
mod daemon;
mod proto;
mod theme;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = <cli::Cli as clap::Parser>::parse();
    if args.verbose {
        println!("{:?}", args);
    }

    match args.cmd {
        cli::Cmd::Daemon => daemon::run(),
        cli::Cmd::Ui => ui::run(),
        cli::Cmd::Pick { element } => {
            let entries = proto::list(u8::MAX, 0)?;
            Ok(proto::set(
                entries.get(element).ok_or("no such element")?.id,
            )?)
        }
        cli::Cmd::List => {
            for (i, entry) in proto::list(u8::MAX, 0)?.iter().enumerate() {
                let preview: String = if entry.mime.starts_with("text/") {
                    String::from_utf8_lossy(&entry.data)
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .chars()
                        .take(80)
                        .collect()
                } else {
                    String::new()
                };
                println!(
                    "{i}: [{}, {} bytes] {preview}",
                    entry.mime,
                    entry.data.len()
                );
            }
            Ok(())
        }
    }
}
