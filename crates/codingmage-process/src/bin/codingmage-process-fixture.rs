//! Deterministic subprocess fixture; excluded from release packaging by policy.

use std::{env, fs, io::Read, process::Command, thread, time::Duration};

fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let Some(mode) = arguments.first().map(String::as_str) else {
        std::process::exit(64);
    };
    match mode {
        "args" => println!("{:?}", &arguments[1..]),
        "env" => {
            let requested = arguments.get(1).map_or("SAFE_VALUE", String::as_str);
            println!(
                "requested={}",
                env::var(requested).unwrap_or_else(|_| "absent".to_owned())
            );
            println!(
                "home={}",
                env::var("HOME").unwrap_or_else(|_| "absent".to_owned())
            );
            println!("cwd={}", env::current_dir().unwrap().display());
        }
        "stdin" => {
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input).unwrap();
            std::io::Write::write_all(&mut std::io::stdout(), &input).unwrap();
        }
        "output" => {
            let count = arguments.get(1).unwrap().parse::<usize>().unwrap();
            print!("{}", "x".repeat(count));
            eprint!("{}", "y".repeat(count));
        }
        "sleep" => {
            let millis = arguments.get(1).unwrap().parse::<u64>().unwrap();
            thread::sleep(Duration::from_millis(millis));
        }
        "spawn-child" => {
            let pid_path = arguments.get(1).unwrap();
            let millis = arguments.get(2).unwrap();
            let mut child = Command::new(env::current_exe().unwrap())
                .args(["sleep", millis])
                .spawn()
                .unwrap();
            fs::write(pid_path, child.id().to_string()).unwrap();
            let status = child.wait().unwrap();
            std::process::exit(status.code().unwrap_or(1));
        }
        "fail" => {
            let code = arguments.get(1).unwrap().parse::<i32>().unwrap();
            std::process::exit(code);
        }
        _ => std::process::exit(64),
    }
}
