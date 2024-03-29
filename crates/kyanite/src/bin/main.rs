use kyac::Backend;
use kyanite::{installed, Commands};
use std::{
    io::{BufRead, BufReader, Write},
    process::Stdio,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = kyanite::cli();
    kyanite::init_logger(cli.verbose)?;
    if cli.llvm
        && !(installed("llc", "try installing LLVM") && installed("clang", "try installing LLVM"))
    {
        return Ok(());
    }
    let backend = if cli.llvm {
        Backend::Llvm
    } else {
        Backend::Kyir
    };
    log::debug!("using backend: {:?}", backend);
    let cli = kyanite::cli();
    if cli.gc_always {
        std::env::set_var("KYANITE_GC_ALWAYS", "1");
    }
    match cli.command {
        Commands::Run { path } => {
            let dir = tempfile::tempdir().unwrap_or_else(kyanite::fatal);
            let exe = kyanite::build(path, &dir, &backend);
            log::info!("running ./{exe}");
            let child = std::process::Command::new(format!("./{exe}"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap_or_else(kyanite::fatal);
            let reader = BufReader::new(child.stdout.unwrap());
            let mut stdout = std::io::stdout();
            for line in reader.lines() {
                writeln!(&mut stdout, "{}", line.unwrap()).unwrap();
            }
            Ok(())
        }
        Commands::Build { path } => {
            let dir = tempfile::tempdir().unwrap_or_else(kyanite::fatal);
            let exe = kyanite::build(path, &dir, &backend);
            log::info!("built ./{exe}");
            Ok(())
        }
        Commands::Version => {
            println!(
                "kyanite {} (kyac: {}, runtime: {})",
                kyanite::VERSION,
                kyac::VERSION,
                runtime::VERSION,
            );
            Ok(())
        }
    }
}
