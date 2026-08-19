# Configuration

## Authority Roots

Configuration version 1 requires absolute existing target, scratch, and state directories. Roots may not overlap. The task source is a relative normal path inside the target repository, and parent discovery defaults to disabled.

## Profiles And Gates

Agent profiles name a stable identifier, provider, and model. Gate commands contain an absolute executable and literal argument vector; shell strings are not accepted. At least one profile and one gate are required.

## External Capabilities

Network, feature-branch push, issue synchronization, and draft pull requests are separately denied by default. Push, issues, or pull requests cannot be enabled while network is denied. Publication mode must agree exactly with the grants.

Configuration contains references and policy, not raw credentials. Native credential-store integration remains outside the current CLI workflow.
