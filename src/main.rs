fn main() -> std::process::ExitCode {
    match greentic_bundle::main_entry() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{}", greentic_bundle::render_cli_error(&err));
            std::process::ExitCode::FAILURE
        }
    }
}
