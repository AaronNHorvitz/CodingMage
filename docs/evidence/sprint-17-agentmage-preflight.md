# Sprint 17 AgentMage Pilot Evidence

- **Status:** Preparation, fake-agent dry run, and first live supervised unit complete; five-unit and unattended pilots remain open
- **AgentMage source checkpoint:** branch `claude/sprint-41-verification`, commit `caf22663`
- **CodingMage pilot build:** commit `c3b3103`
- **Executed:** 2026-08-19 on Fedora Linux with Rust 1.95.0

## Authorization And Selection

The real AgentMage checkout was clean and synchronized at `caf22663`. Its configuration denies network, push, issue, pull-request, merge, and publication authority. The run specification selected exact open sub-task `42.1.3.2`, limited Claude's write authority to `platforms/linux/src/repository_safety.rs`, and used `candidate_only` so the partially external task could not be marked complete.

CodingMage now parses AgentMage's checkbox sprint and story headings, acceptance-criterion identifiers, and legacy task suffixes without rewriting the source. `codingmage doctor` parsed 169 sprints, 203 stories, and 4,544 items, reported a clean target, and found no unsafe checkout feature.

## Disposable Dry Run

The committed `codingmage-soak` integration test remains the source-independent dry run. It materializes a clean AgentMage-shaped fixture, selects one canonical unit, executes the fake implementation and senior-review lifecycle in exact order, and requires zero target-repository mutation.

## Live Unit

The first live run used Claude Opus as implementer through an existing subscription login. CodingMage passed only the four approved login-discovery references plus compiled `PATH=/usr/bin:/bin`, strict empty MCP configuration, file-only worktree tools, denied network tools, and no Git authority. Claude changed only the declared file. The coordinator reconciled the reported path and created candidate commit `3208b98d` on a local CodingMage integration branch while the active AgentMage checkout remained untouched.

Two fail-closed defects were found before the successful implementation call: the current Claude CLI requires `{"mcpServers":{}}` rather than `{}` for an empty strict MCP registry, and its sandbox requires a fixed executable path to discover installed Bubblewrap and Socat. Both defects were corrected in CodingMage with regression coverage. No failed attempt changed AgentMage source.

The initial local-gate profile named nonexistent `/usr/bin/cargo`, so the durable run stopped before gate execution and released its owned resources. The private profile was corrected to the pinned Rust 1.95 toolchain with Cargo offline mode. Rustfmt then found three formatting-only differences. A human-supervised mechanical Rustfmt correction produced exact candidate `e7a4aafbe4ba1604e4ddfdd80ca1c7f747b7caa0`; CodingMage's automatic correction loop remains unimplemented and is not claimed.

## Verification And Review

The corrected exact candidate passed:

- all 19 `repository_safety::tests` cases with ignored live fixtures included and one test thread;
- strict Clippy for all `agentmage-platform-linux` targets;
- workspace Rustfmt check; and
- `git diff --check` against source commit `caf22663`.

The configured Codex CLI then reviewed only `caf22663..e7a4aafb` in read-only mode. It verified the exact commit lineage and single-file scope and returned `pass` with zero findings. A deliberately mistyped target SHA in the first manual review packet was refused as missing rather than silently substituted; the corrected exact-SHA packet passed.

## Preserved Limits

The pilot branch is local only. It was not pushed, merged, or used to change AgentMage's canonical task checkbox. The original durable run did not reach its checkpoint because the gate profile failed before execution; the subsequent gate and review evidence was supervised manually and is stated as such. This closes one live implementation-and-review unit, not the five-unit campaign, unattended workflow, automatic correction, service execution, sustained soak, external blocker, or publication gates.
