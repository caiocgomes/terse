use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cwd = std::env::current_dir().expect("current directory must be accessible");
    let code = terse_cli::run(args, &cwd);
    ExitCode::from(code as u8)
}
