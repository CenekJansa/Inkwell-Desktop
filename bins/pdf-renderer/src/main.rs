//! Isolated PDF rendering sidecar entry point.

use std::process::ExitCode;

use inkwell_pdf_renderer::{DistributionEngine, serve};

fn main() -> ExitCode {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let result = serve(&mut stdin.lock(), &mut stdout.lock(), DistributionEngine);

    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        // Diagnostics are intentionally omitted: stdout is protocol-only and
        // protocol failures may contain sensitive document context.
        ExitCode::FAILURE
    }
}
