mod cli;
mod daemon;
mod proto;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = <cli::Cli as clap::Parser>::parse();
    if args.verbose {
        println!("{:?}", args);
    }

    match args.cmd {
        cli::Cmd::Daemon => daemon::run(),
        cli::Cmd::Ui => {
            if let Some(id) = ui::run(proto::list(u8::MAX)?)? {
                proto::set(id)?;
            }
            Ok(())
        }
        cli::Cmd::Pick { element } => {
            let entries = proto::list(u8::MAX)?;
            Ok(proto::set(
                entries.get(element).ok_or("no such element")?.id,
            )?)
        }
        cli::Cmd::List => {
            for (i, entry) in proto::list(u8::MAX)?.iter().enumerate() {
                let preview: String = String::from_utf8_lossy(&entry.data)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(80)
                    .collect();
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
