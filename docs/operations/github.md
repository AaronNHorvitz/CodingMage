# GitHub

GitHub synchronization is deny-first and optional. Account, host, repository, feature branch, and protected branch are exact identities. Issue read/write, pull-request read/write, comments, and branch push are independent grants.

CodingMage owns only marker-bounded issue sections and preserves human text outside them. Remote checkboxes never override canonical local task state. Writes use idempotency keys, timeouts reconcile by exact key, and one version conflict may refetch and preserve concurrent human edits.

Only configured nonprotected feature branches may be pushed after local gates. Pull requests are draft-only, automated review comments are labeled, and no merge, release, settings, secret, Actions-administration, force-push, or branch-delete operation exists.

The fake transport passes. Authenticated disposable-repository evidence remains open.
