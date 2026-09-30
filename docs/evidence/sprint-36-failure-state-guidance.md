# Sprint 36 native observation failure guidance

Task 36.3.2.2 remains open. The existing coordinator command boundary now supplies an explicit
presentation category for known provider authentication, capability/process, executable,
permission, timeout, oversized and output-contract failures. Contract errors distinguish
malformed output from a mismatched campaign identity and an unsupported schema. Unknown provider
codes remain a neutral failed request. No category grants authority or claims a retry succeeded.

Preflight and repository diagnosis use the same bundled state labels. Each displays the stable
cause code, what happened, its effect on the current observation and a next action. A failed
preflight without a report cannot admit. A retained preflight report remains inspectable and
cannot admit until a new successful, live preflight; this keeps Decision 0033's boundary. A failed
diagnosis refresh retains the earlier result as stale and tells the owner to refresh before
relying on it. Provider sign-in happens through the configured provider CLI outside CodingMage;
the interface never signs in or substitutes a provider.

The shell labels an available worker as a configured coordinator command, rather than claiming
the command is currently ready after a later execution failure. The screen failure and freshness
remain visible until a successful observation replaces them.

The focused fixtures inject stable error codes and malformed bytes through the same response
binding as the worker, after opening a disposable repository with the real coordinator. They
assert the labels, recovery, failed or stale freshness and absence of campaign state. This is
synthetic fault-injection evidence, not real-provider authentication or desktop qualification.

The first cumulative native UI run found one existing shell assertion tied to the established
malformed-output explanation. The new heading had replaced that sentence. Restoring the sentence
keeps its meaning and adds the explicit state; the first failed result is retained, and final
checks will use the corrected tree.

## Verification disposition

The focused synthetic preflight-state test passed 1/1, and the corrected focused shell
malformed-output test passed 1/1. The first cumulative native UI run passed every target through
Setup but failed one established shell assertion (11/12) because its explanatory sentence had
changed; that sentence was restored and the assertion now also checks the explicit state and
effect. The final corrected-tree software-rendered native UI suite passed 113/113. Strict
workspace Clippy with warnings denied passed. Formatting, documentation, architecture, inventory
no-write and diff whitespace checks passed.

The Python suite ran 42 tests: 41 passed and the sole failure remains CM-R01.6's eight previously
drifted source-bound inputs. No candidate-construction approval, binding or digest was renewed.
The reviewed inventory has 1,733 items and 825 explicit gaps: three new Rust presentation
surfaces, no removed semantic surface or closed gap, six shifted line identifiers and 363
heuristic test-mapping changes. Its mappings do not qualify acceptance. Independent exact-commit
review remains pending. No human, live-provider, installed-desktop or release gate is closed.
