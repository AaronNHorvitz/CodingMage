# Sprint 17 Controlled Target Pilot Evidence

- **Status:** Preparation, fake-agent dry run, and first live supervised unit complete; five-unit and unattended pilots remain open
- **CodingMage pilot build:** commit `c3b3103`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0

## Authorization And Selection

The authorized target checkout was clean and synchronized at an exact source commit. Its configuration denied network, push, issue, pull-request, merge, and publication authority. The run specification selected one exact open sub-task, limited Claude's write authority to one declared source file, and used `candidate_only` so the partially external task could not be marked complete.

CodingMage parsed the target's checkbox sprint and story headings, acceptance-criterion identifiers, and legacy task suffixes without rewriting the source. `codingmage doctor` reported a clean target and found no unsafe checkout feature.

## Disposable Dry Run

The committed `codingmage-soak` integration test remains the source-independent dry run. It materializes a clean controlled-target fixture, selects one canonical unit, executes the fake implementation and senior-review lifecycle in exact order, and requires zero target-repository mutation.

## Live Unit

The first live run used Claude Opus as implementer through an existing subscription login. CodingMage passed only the four approved login-discovery references plus compiled `PATH=/usr/bin:/bin`, strict empty MCP configuration, file-only worktree tools, denied network tools, and no Git authority. Claude changed only the declared file. The coordinator reconciled the reported path and created a candidate commit on a local CodingMage integration branch while the active target checkout remained untouched.

Two fail-closed defects were found before the successful implementation call: the current Claude CLI requires `{"mcpServers":{}}` rather than `{}` for an empty strict MCP registry, and its sandbox requires a fixed executable path to discover installed Bubblewrap and Socat. Both defects were corrected in CodingMage with regression coverage. No failed attempt changed target source.

The initial local-gate profile named nonexistent `/usr/bin/cargo`, so the durable run stopped before gate execution and released its owned resources. The private profile was corrected to the pinned Rust 1.95 toolchain with Cargo offline mode. Rustfmt then found three formatting-only differences. A human-supervised mechanical Rustfmt correction produced an exact candidate commit; CodingMage's automatic correction loop remains unimplemented and is not claimed.

## Verification And Review

The corrected exact candidate passed:

- all 19 repository-safety cases with ignored live fixtures included and one test thread;
- strict Clippy for all affected targets;
- workspace Rustfmt check; and
- `git diff --check` against the exact source commit.

The configured Codex CLI then reviewed only the exact source-to-candidate range in read-only mode. It verified the exact commit lineage and single-file scope and returned `pass` with zero findings. A deliberately mistyped target SHA in the first manual review packet was refused as missing rather than silently substituted; the corrected exact-SHA packet passed.

## Preserved Limits

The pilot branch was local only. It was not pushed, merged, or used to change the target's canonical task checkbox. The original durable run did not reach its checkpoint because the gate profile failed before execution; the subsequent gate and review evidence was supervised manually and is stated as such. This closes one live implementation-and-review unit, not the five-unit campaign, unattended workflow, automatic correction, service execution, sustained soak, external blocker, or publication gates.
