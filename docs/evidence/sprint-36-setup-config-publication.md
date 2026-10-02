# Public guided configuration write: command increment

Decision 0074 adds the public `setup-write-config` command as the coordinator
half of the remaining guided configuration boundary. It accepts an explicit
existing Git repository and external destination, bounded private TOML on
stdin and explicit overwrite consent. The existing typed `Config` schema and
loader validate the held candidate; the guarded publisher checks repository
authority, Git HEAD and scratch/state root identities at both publication
boundaries. Overwrite first binds the existing valid configuration's repository
and held bytes, refusing a foreign repository or changed source before
publication. The receipt contains no configuration text or credential value.
Before returning success, the command checks the named destination through a
held no-follow regular-file descriptor against the exact submitted bytes and
rechecks repository and root authority; a mismatch is an uncertain write.

Missing roots are created only as private direct children of the checked
configuration parent. An error after root creation can leave empty roots for
inspection; no automatic cleanup guesses ownership. Existing roots elsewhere
must satisfy repository authorization and guarded path checks. No success is
reported for uncertain post-publication results.

The native `ConfigForm::write` remains an in-process file effect and is not
represented as using this command. Its migration needs a private-input worker,
submission-bound command preview, receipt and destination verification, and
window-close uncertainty handling. Task 36.3.2.2 remains open. Disposable
tests and static gate results for the exact command source do not establish
human desktop or live-provider qualification.

## Verification disposition

- The final-source disposable command tests passed 2/2; neighboring Setup
  authorization and campaign tests passed 2/2 and 9/9. The broader CLI
  all-target run passed 72 tests with two explicit sustained-qualification
  ignores. That binary was built before the final held-output check; the
  focused final-source run covers the affected publication boundary.
- Strict workspace Clippy with warnings denied, Cargo formatting, documentation
  checks, architecture checks and Git whitespace checks passed. The initial
  final-source Clippy run found an owned 107-line function; extracting final
  publication verification fixed it and the rerun passed.
- The full Python suite ran 46 tests: 45 passed and the pre-existing CM-R01.6
  source-bound freshness test failed with eight `input-drift` inputs. The
  receipt and source hashes were not renewed. The inventory generator passed
  after source review with 1,808 items and 825 explicit gaps. It adds two
  normalized public APIs, removes none, changes no applicability, and updates
  11 heuristic test-name mappings; a mapping is not proof of coverage.
- The command has no native client yet. Missing roots created before a later
  failure may remain as empty private directories. A post-publication
  authority failure returns `uncertain_write`; the operator must inspect the
  named destination before another attempt.
