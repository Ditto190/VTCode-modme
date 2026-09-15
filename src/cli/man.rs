use std::io::Write;

use anyhow::{Context, Result};
use vtcode_core::cli::ManPageGenerator;
use vtcode_core::utils::file_utils::write_file_with_context_sync;

pub async fn handle_man_command(command: Option<String>, output: Option<std::path::PathBuf>) -> Result<()> {
    let content = match command.as_deref() {
        Some(cmd) => ManPageGenerator::generate_command_man_page(cmd)
            .with_context(|| format!("Failed to generate man page for {cmd}"))?,
        None => ManPageGenerator::generate_main_man_page().context("Failed to generate main man page")?,
    };

    if let Some(path) = output {
        write_file_with_context_sync(&path, &content, "man page")?;
        print_stdout_ignore_broken_pipe(&format!("Wrote man page to {}", path.display()))?;
    } else {
        print_stdout_ignore_broken_pipe(&content)?;
    }

    Ok(())
}

/// Write one block to stdout, treating a closed downstream pipe as success.
///
/// `println!` panics on `EPIPE` (e.g. `vtcode man | head`, quitting `less`
/// early). Piping large generated output is normal, so ignore
/// `ErrorKind::BrokenPipe` and only surface real I/O failures.
fn print_stdout_ignore_broken_pipe(text: &str) -> Result<()> {
    match writeln!(std::io::stdout().lock(), "{text}") {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error).context("failed to write to stdout"),
    }
}
