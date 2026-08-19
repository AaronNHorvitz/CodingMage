//! `CodingMage` command-line entry point.

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
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
