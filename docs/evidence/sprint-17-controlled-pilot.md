# Sprint 17 Controlled Target Pilot Evidence

- **Status:** Preparation, fake-agent dry run, first live unit, five-unit supervised qualification, owner-approved background enablement, and one unattended corrected story complete; blocker/quota and handoff evidence remain open
- **Initial CodingMage pilot build:** commit `c3b3103`
- **Five-unit qualification build:** commit `2631265`
- **Executed:** 2026-08-19 and 2026-08-21 on Fedora Linux with Rust 1.95.0

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

## Five-Unit Supervised Qualification

Commit `1c3a54f` added an explicit `codingmage-soak` qualification runner that materializes five separate clean repositories. Each repository contains one canonical open sub-task, one owned text file, one unowned expected file, and an exact-content `/usr/bin/cmp` gate. The runner uses the production `codingmage run` path with existing-login discovery, local-only publication, a Claude `opus` implementation profile, and a Codex `gpt-5.6-sol` read-only review profile. Routine test commands compile the runner but never invoke a live provider.

The first prequalification attempt found a real fail-closed compatibility defect before candidate creation. Claude Code 2.1.136 interpreted CodingMage's single-leading-slash file-tool permission rules as project-relative paths and denied both authorized write tools. CodingMage released the worktree and returned `blocked` with no candidate or completion commit. Commit `2631265` changed only the permission-rule encoding to Claude's documented double-leading-slash absolute form, retained normal absolute paths for the OS sandbox, and added regression assertions for both representations. All eight focused adapter tests and strict Clippy passed before qualification resumed.

The post-correction qualification completed five of five units:

| Unit | Run | Candidate | Completion | Result |
|---|---|---|---|---|
| 1 | `run-0450fbb27ac1c964ef703dab76e52162` | `b48370a2dfafeec4d2a5e8cee1980e4921b26ce5` | `54093d2626da2005265bc4a7edede89c4fc2bd93` | complete, review pass |
| 2 | `run-747efe8c990d99d366c3ef5693bb3b0d` | `968eb4be416fc47a0c94d4be388a4e1212f56c94` | `24eb40b00c1ccaab55af5594818bd98b6ae8c460` | complete, review pass |
| 3 | `run-dc2f40b21fdce735901a3cb5d63d7406` | `159e8d70c7e933327c9fd6658266ec7c6595750e` | `1bc21a23bdf4cec192b9e38be2debe0ae531f777` | complete, review pass |
| 4 | `run-a009fb524cd0a96ee0b50cef2093fa9c` | `53d4bb2f3240c870514c059242916d494eb75f78` | `cc5395ad118aad180328b6ea8b14e601acf09844` | complete, review pass |
| 5 | `run-ebc601ce911d8e609eeeff3199105499` | `5b3af7da4631d6925da3577f810b02a6ce5c754f` | `a7cb2783ede545248b1c44da9b2230c3b3775908` | complete, review pass |

Every unit consumed exactly two provider attempts and eleven total process invocations, with zero malformed-report repairs and zero correction rounds. Each active checkout retained its exact original HEAD, porcelain status, task source, and artifact bytes. Each candidate retained the unchecked task source and the exact expected artifact; each separate completion commit was the candidate's direct child and changed the canonical sub-task marker. Each disposable repository ended with exactly its active worktree plus one retained local integration branch, an empty scratch root, and one terminal `release:succeeded` journal. A second process inspection found no qualification-owned CodingMage, Claude, or Codex process. The release binary SHA-256 was `a4d774eaf858d37043b29f15e42c880c8e7e2aeffaec790ba557d97033bc9425`.

The disposable fixtures and content-minimized journals remain under the operator's private CodingMage qualification state for review. No fixture branch was pushed, merged, or published. The five-unit evidence qualifies supervised bounded operation; by itself, it neither authorized background execution nor established an unattended production-target claim.

On 2026-08-21, after receiving the five-unit results and retained limitations, the product owner explicitly approved the retained evidence and authorized background execution for the next bounded unattended pilot. That authorization is limited to the disposable pilot and does not grant target-repository publication, merge, release, network, or external-infrastructure authority.

## Unattended Corrected Story

Commits `595df9b` and `03d4674` added and corrected a dedicated one-story unattended qualification runner. It used the production serial campaign coordinator with one pod, one accepted-unit ceiling, local-only publication, a real Claude `opus` implementer, and deterministic fake Codex lead and reviewer adapters. The fake reviewer required one exact second-line attestation after the initial valid implementation. This isolates correction and resume behavior from reviewer variability; the five-unit evidence above separately establishes live Codex review behavior.

The first run completed the story but exposed an incorrect harness assumption: serial campaigns intentionally retain one campaign-root worktree for durable reobservation, while implementation pod worktrees are released. The harness rejected that run rather than treating expected retained state as a leak. Commit `03d4674` changed the postcondition to require exactly one active checkout plus one Git-bound campaign worktree, and an empty campaign pod-scratch root.

The fresh post-correction run completed without operator intervention:

- base commit `ff39399595a1fd024d37a44832339a8ce34f90de` remained checked out cleanly on `main`;
- campaign head `8b1e986070677e7792183ca458bd9e81d764eca6` contained the exact reviewed artifact and mechanical task completion;
- five provider attempts and sixteen total process invocations covered lead selection, initial implementation, first review, resumed correction, and passing re-review;
- exactly one correction round completed, with zero malformed-report repairs;
- the durable checkpoint ended in `complete` with one completed unit, no active unit, no pending integration, and no blocker;
- the implementation pod worktree was released, the single campaign-root worktree remained bound to the exact campaign head, and the active checkout was byte-for-byte preserved; and
- a second process inspection found no qualification-owned CodingMage or Claude process.

The retained state contained 42,437 bytes and the observed provider and gate output totaled 15,209 bytes, both within the operator-authorized 16 MiB ceilings. The run used no publication, network, issue, pull-request, merge, release, or external-infrastructure authority.

## Preserved Limits

The original pilot branch was local only. It was not pushed, merged, or used to change the target's canonical task checkbox. The original durable run did not reach its checkpoint because the gate profile failed before execution; the subsequent gate and review evidence was supervised manually and is stated as such. The later disposable campaign closes the five-unit supervised requirement after the permission-rule correction, but not background enablement, an unattended workflow, service execution, sustained production-target soak, external blocker handling, or publication gates.
