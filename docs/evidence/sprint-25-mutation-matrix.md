# Sprint 25 Mutation Matrix

## Scope

This matrix binds each durable authority and evidence class to executable type-valid field mutation,
unknown-field, identity, ordering, or integrity tests. Mutation evidence is fail-closed: changed data
must be rejected before it broadens authority or attests to an unobserved effect.

| Class | Exact executable evidence |
| --- | --- |
| Campaign authority | `codingmage_campaign::tests::every_campaign_authority_field_is_digest_bound_or_rejected`; `codingmage_campaign::tests::proposal_mutation_and_stale_source_are_rejected` |
| Lead report | `codingmage_campaign::tests::every_mixed_disposition_payload_is_rejected`; `codingmage_campaign::tests::hostile_lead_authority_expansion_corpus_fails_before_admission` |
| Provider report | `codingmage_agent::tests::malformed_unordered_wrong_session_and_unknown_fields_fail_closed`; `codingmage_claude::tests::provider_result_parsing_rejects_unknown_report_fields_and_maps_quota`; `codingmage_codex::tests::report_rejects_stale_commits_duplicate_ids_and_escaping_paths` |
| Review | `codingmage_review::tests::material_correction_cannot_be_solely_self_reviewed`; `codingmage_review::tests::lifecycle_deduplicates_and_requires_proven_verification` |
| Lease and scheduler snapshot | `codingmage_campaign::team::tests::policy_and_snapshot_mutations_fail_closed`; `codingmage_runtime::team_state::tests::unbound_or_mismatched_document_fails_closed` |
| Journal | `codingmage_state::journal::tests::detects_each_field_mutation_and_unknown_field`; `codingmage_state::journal::tests::detects_torn_duplicate_reordered_and_chain_broken_records` |
| Checkpoint | `codingmage_runtime::campaign_state::tests::checkpoint_round_trips_and_rejects_tampering`; `codingmage_runtime::campaign_state::tests::checkpoint_refuses_self_consistent_outcome_projection_mutations` |
| Gate evidence | `codingmage_gate::runner::every_evidence_field_mutation_breaks_integrity`; `codingmage_gate::runner::exit_zero_without_expected_assertion_cannot_pass` |
| Integration | `codingmage_runtime::team_integration::tests::serialized_integration_completes_from_fresh_and_every_durable_git_boundary`; `codingmage_git::integration::tests::non_descendant_and_unowned_changes_refuse_before_mutation` |
| Publication | `codingmage_runtime::team_publication::tests::campaign_branch_publication_rejects_mismatched_adapter_identity`; `codingmage_github::tests::issue_fields_cannot_inject_ownership_markers`; `codingmage_github::tests::timeout_is_reconciled_by_key_and_never_blindly_replayed` |
| Package manifest | `tests.test_release_tools.ReleaseToolsTest.test_every_build_manifest_field_mutation_fails_closed`; `tests.test_release_tools.ReleaseToolsTest.test_checksum_and_archive_traversal_fail_closed` |

## Package Correction

The rootless installer now requires a closed `BUILD-MANIFEST.json` with schema version, release
version, source commit, source epoch, lockfile digest, binary digest, credential and runtime-state
absence claims, and Linux evidence boundary. It independently validates every field and requires
the checksum manifest to declare every extracted file exactly once. Mutating any manifest field,
adding an unknown field, changing a checksum, omitting a declared file, adding an undeclared file,
or traversing outside the archive root fails before installation.

A fresh two-build package passed the tightened installer, checksum verification, binary version
execution, and guarded removal. This is unsigned local package evidence; signature and public-
artifact verification remain separate human release gates.
