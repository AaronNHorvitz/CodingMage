# Sprint 26 Package C Live Qualification

**Date:** 2026-08-26

**Installed source commit:** `82383835d5f34dddac81e07b4394a7c5f91f62e4`

**Archive SHA-256:**
`0e99a870836a7785118bc31fd1d62ef549aec62cf430152f8674819ecb5bd525`

**Installed binary SHA-256:**
`e5c5986bef8b3a13a8cbfeec7567a4e4c3e664f4ab0ccd5db04b8fd60169e629`

## Boundary

Package C created fresh private state and scratch roots through its own deny-first `init` path. Its
configuration denied network, push, issue, pull-request, task-merge, destination-merge, and
publication effects. `doctor` passed against the clean no-hardlinks target at exact commit
`6377b296448d16a20f6f63e6d7cc1162e87a05a2`.

The live units used the installed Package C coordinator, an existing-login Claude implementation
provider, an exact executable Codex reviewer, and six deterministic gates: Rust formatting, offline
npm installation, documentation validation, complete locked workspace tests, strict workspace
Clippy, and Git diff integrity.

## Ownership-Contract Unit

Run `run-2cc88ae161f7ade27b242651905e8d0b` attempted Sub-task `1.2.1.2`. The run:

- produced an isolated initial candidate and passed all configured gates;
- received valid changes-required reviews and applied separately gated corrections;
- retained an immutable green candidate across a failed Codex review and resumed it by exact run
  identity without replaying initial implementation;
- retained and resumed prepared Claude corrections after the 15-minute correction deadline;
- kept provider attempts independently scoped to initial implementation, each correction round and
  parent session, and each immutable review candidate; and
- stopped as `recoverable_failure` at the configured eight-correction-round ceiling with candidate
  commit `78bdaac06c542a2132ffae5527783cfa25d0bf52` and no completion commit or review pass.

The unit did not complete and makes no supervised-success claim.

## Schema-Evolution Planning Unit

Run `run-c234c12e170c625f17485486e5c35da2` attempted the narrower Sub-task `1.2.4.2`. Its initial
candidate and six corrected immutable candidates passed the complete deterministic gate set. Valid
Codex reviews returned bounded changes-required verdicts through correction round six.

The final retained green candidate was
`9a4d36979f27c137ae71bfd6b56afaea2eb24034`. Three independent Codex review invocations failed
before returning a valid verdict. A fourth exact-run invocation reran deterministic gates, retained
the same candidate, and moved from review transition to owned-resource release without spawning a
reviewer. The integrity-bound ledger remained at exactly three attempts for that candidate. The
task stayed open and no completion commit was produced.

This unit also did not complete and makes no supervised-success claim.

## Repository Preservation

The no-hardlinks frozen target remained clean at exact commit
`6377b296448d16a20f6f63e6d7cc1162e87a05a2` throughout both units. No target integration or
publication effect occurred. Qualification worktrees and failed candidates remain isolated for
review.

The separate active downstream checkout began the campaign at
`626a92146ff9769f83d36a599a813befb98c819c` with an empty porcelain status. During the live
qualification, a separate development session created ordinary commits in that checkout and
advanced it to `2db805be1cb8ab6ea67c5c59ee87b8b2dfd95e43`, still with an empty porcelain status. CodingMage
did not address, adopt, reset, or modify that checkout, but the original active-checkout fingerprint
therefore did not remain constant. Under the frozen-target evidence policy, no unchanged-active-
checkout claim is made for this interval.

## Disposition

- Sub-tasks `26.1.3.6` and `26.1.7.3` remain open because no fresh supervised unit passed.
- The three-outcome unattended pilot was not started because its supervised prerequisite did not
  pass.
- The approximately ten-task controlled soak remains open.
- No Sprint 26 acceptance criterion or gate is closed by this evidence.
- The live runs prove bounded recovery and refusal behavior, but they also show that reviewer
  availability and review convergence remain qualification blockers for unattended progression.
