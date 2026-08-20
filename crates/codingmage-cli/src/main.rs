//! `CodingMage` command-line entry point.

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments == ["__process-guard"] {
        std::process::exit(codingmage_process::guard_entry());
    }
    match codingmage_cli::run(&arguments) {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
        }
        Err(error) => {
            eprintln!("{}", error.code());
            std::process::exit(error.exit_code());
        }
    }
}
