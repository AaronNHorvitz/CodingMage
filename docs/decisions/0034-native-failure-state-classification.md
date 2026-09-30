# ADR 0034: Classify native observation failures by known cause

- **Status:** Accepted
- **Date:** 2026-09-29
- **Decision owners:** CodingMage implementation

## Context

The native preflight screen explained stable backend codes but headed most failures as a generic
preflight failure. Repository diagnosis likewise showed an error without consistently stating
whether an earlier observation remained usable. This obscured provider sign-out, missing
capability, malformed output and a failed refresh with retained data. A contract failure can also
mean a different campaign identity or an unsupported schema, which must not be described as
malformed output.

## Decision

Classify only known backend variants and exact stable provider codes into presentation states.
Keep unknown and future codes as an unclassified failed request. Keep malformed data, identity
mismatch, unsupported schema, oversized output, missing executable and permission denial
distinct. Use one bundled English message catalogue for the state labels and observation effects.
Preflight and repository diagnosis display the stable cause code, an explanation, the effect on
the current observation and a recovery action. Earlier successful values remain visible as stale
after a failed refresh. A preflight failure never supplies a new admissible report.

This classification is presentation only. The coordinator remains the source of authority and
the UI does not infer a provider login, a completed campaign action or a successful retry from a
failure category. No provider, command, credential path, dependency or effect authority changes.

## Consequences

An unknown future provider code receives neutral wording until its contract is reviewed. The
state catalogue is still incomplete across other screens, and full text externalization and
desktop accessibility qualification remain open under Story 36.3.
