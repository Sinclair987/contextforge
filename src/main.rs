use std::process::ExitCode;

fn main() -> ExitCode {
    match contextforge::cli::run(std::env::args_os()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
