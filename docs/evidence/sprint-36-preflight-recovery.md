# Sprint 36 preflight permission and admission recovery increment

Task 36.3.2.2 remains open. The native backend now distinguishes a present coordinator file
that the operating system refuses to execute (`codingmage.ui.permission_denied`) from a missing
sibling executable and other process-start failures. The preflight screen names the cause,
states that the request did not admit or start a campaign, and tells the owner to restore the
installed file's execute permission outside the app before retrying. No installer or permission
change is attempted by the interface.

The last successful report is retained for inspection while a refresh is pending and after a
failure. It cannot be used to record a new admission in those states or after the existing
observation window marks it stale. The UI disables the admission button, and the direct app
method checks the same condition. A successful fresh preflight clears the failure and makes the
reviewed digest available again. Coordinator start continues to perform its own authority
validation.

The disposable repository regression uses a copied real coordinator executable and a ready
campaign. It observes a successful preflight, removes execute permission from that copy, proves
that the failed refresh retains the digest but cannot admit, checks the permission-denied screen,
then restores the original permission and proves fresh preflight recovery and admission. The
campaign is never started by these steps. This fixture does not qualify installed desktops,
provider accounts or live model behavior.

The first focused run placed the copied coordinator above the disposable repository, which the
repository guard correctly refused. Moving it to a sibling `bin` directory passed the focused
recovery test. The first cumulative run then exposed an old integration helper that accepted a
retained report as completion of a new preflight request; its test failed under the new pending
guard. The helper now waits until that request finishes. Both failed attempts are retained in
private runtime evidence and neither is counted as a passing gate.

The initial strict workspace Clippy run also found that the expanded preflight renderer exceeded
the repository's function-length limit. Its progress and failure display was extracted into a
separate private method without changing the state rules; final checks must use that tree.

## Verification disposition

The corrected focused permission-recovery case and the corrected retained-report admission
case each passed 1/1. The software-rendered native UI all-target suite passed 110/110 both
before and after the private renderer extraction. Strict workspace Clippy passed on the final
source tree. Python unittest ran 42 cases: 41 passed and the sole failure remains CM-R01.6's
eight source-bound input drifts; no binding or digest was renewed. Formatting,
documentation, architecture and staged diff whitespace checks passed. The reviewed and regenerated
verification inventory has 1,730 items and 825 explicit gaps: one new stable UI error-code
surface, no removed surface or closed gap. Its test mappings are heuristic, not qualification.
Independent review is pending. No human, live-provider, source-bound candidate-construction or
release gate is closed.
