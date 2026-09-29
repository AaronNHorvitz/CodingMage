# Sprint 36 first-run orientation increment

The empty native Overview now explains the supported first step: open an existing repository
configuration or create one in Setup to inspect the task source and readiness. It identifies the
configured local state and scratch directories, explains that providers use their own existing
sign-in, and links to Setup and offline Help. Opening the screen does not start a campaign or
select a provider or model. The written missing-coordinator, minimum-window test traverses both
links and checks that no repository or diagnosis was opened.

This is one open Task 36.3.2.2 increment, not a complete first-run, state catalogue or screen
qualification. Storage paths, provider profiles and preflight results are still provided by the
existing configuration and coordinator contracts. Language externalization, real desktop/Orca
verification, human trials and live-provider qualification remain open.

## Verification disposition

The focused first-run test passed 1/1, strict workspace Clippy passed, and the software-rendered
native UI all-target suite passed 98/98. Python unittest passed 41/42, with only the existing
CM-R01.6 eight-input evidence drift. The exact staged diff was inspected; exact-commit
independent review remains pending. No task, acceptance criterion or gate is closed.
