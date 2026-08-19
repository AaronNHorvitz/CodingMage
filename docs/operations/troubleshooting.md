# Troubleshooting

## Stable CLI Codes

- `codingmage.cli.usage`: malformed or incomplete command syntax.
- `codingmage.cli.invalid_argument`: relative, linked, missing, or otherwise invalid selected path.
- `codingmage.cli.config`: configuration failed strict loading.
- `codingmage.cli.repository`: repository authorization or hardened inventory failed.
- `codingmage.cli.plan`: task source failed strict parsing.
- `codingmage.cli.no_ready_work`: no open dependency-ready sub-task exists.
- `codingmage.cli.refused`: initialization would overwrite or broaden authority.
- `codingmage.cli.execution_unavailable`: live orchestration is deliberately disabled.

Errors omit source text, path values, credentials, and provider output. Use `doctor` to obtain redacted repository and configuration facts. Preserve failed state and evidence when reporting a defect.
