//! Parent-failure fixture; excluded from release packaging by policy.

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    path::PathBuf,
};

use codingmage_process::{CancellationToken, ProcessExecutor, ProcessProfile, ProcessRequest};

fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 4 {
        std::process::exit(64);
    }
    let guard = PathBuf::from(&arguments[0]);
    let fixture = PathBuf::from(&arguments[1]);
    let control = PathBuf::from(&arguments[2]);
    let child_pid = arguments[3].clone();
    let vector = vec!["spawn-child".to_owned(), child_pid, "10000".to_owned()];
    let profile = ProcessProfile::new(&fixture, [vector.clone()], []).unwrap();
    let executor = ProcessExecutor::new(&guard, &control).unwrap();
    let request = ProcessRequest {
        arguments: vector,
        working_directory: control.clone(),
        environment: BTreeMap::default(),
        stdin: Vec::new(),
        max_output_bytes: 1024,
        deadline_millis: 30_000,
        max_processes: 4,
        max_open_files: 64,
        expected_exit_codes: BTreeSet::from([0]),
    };
    let result = executor
        .execute(&profile, &request, &CancellationToken::default())
        .unwrap();
    std::process::exit(i32::from(
        result.outcome != codingmage_process::ProcessOutcome::Succeeded,
    ));
}
