//! Muse worker protocol requirements and deterministic fake transcripts.
//!
//! Local specification only for Sub-task 31.1.1.1: the required structured
//! probe/start/continue/cancel/usage behavior a Muse worker must satisfy
//! before admission, plus deterministic fake transcripts built on the
//! provider-neutral [`codingmage_agent`] contract. No CLI is invoked, no
//! authentication material is read, and no live integration is authorized:
//! admitting a worker additionally requires the exact CLI inspection
//! (31.1.1.2), the typed adapter (31.1.1.3), confinement proofs
//! (31.1.1.4), fault fixtures (31.1.1.5), and explicitly authorized live
//! qualification (Story 31.2).

use std::{collections::BTreeSet, fmt};

use codingmage_agent::{AgentCapabilities, AgentCapability};

/// Adapter name a Muse worker must report during the capability probe.
pub const MUSE_PROVIDER: &str = "muse";

/// Required structured behavior for a Muse worker: noninteractive
/// structured output, ordered event streaming, exact session continuation,
/// explicit cancellation, and usage observation. A worker missing any of
/// these fails admission; the gap is a blocker, never a bypass.
#[must_use]
pub fn required_capabilities() -> BTreeSet<AgentCapability> {
    BTreeSet::from([
        AgentCapability::StructuredOutput,
        AgentCapability::EventStream,
        AgentCapability::SessionContinuation,
        AgentCapability::Cancellation,
        AgentCapability::UsageObservation,
    ])
}

/// Reason a reported capability set fails Muse worker admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MuseSpecError {
    /// The probe did not report the Muse adapter name.
    WrongProvider,
    /// The probe reported no version or fingerprint to pin later drift against.
    MissingVersion,
    /// The probe omitted one required structured behavior.
    MissingCapability(AgentCapability),
}

impl fmt::Display for MuseSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongProvider => formatter.write_str("codingmage.muse.wrong_provider"),
            Self::MissingVersion => formatter.write_str("codingmage.muse.missing_version"),
            Self::MissingCapability(capability) => {
                write!(
                    formatter,
                    "codingmage.muse.missing_capability.{capability:?}"
                )
            }
        }
    }
}

impl std::error::Error for MuseSpecError {}

/// Exact installed CLI surface observed for Sub-task 31.1.1.2.
///
/// Only explicitly declared flags and subcommands are pinned: identity
/// comes from `--version`, and structured-protocol markers come from the
/// root and `exec` help text. Anything not declared there is recorded as
/// a blocker by [`MuseCliCapabilities::blockers`], never assumed. No
/// executable path is stored: absolute paths are operational mapping and
/// stay outside the repository.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MuseCliCapabilities {
    /// Exact observed `--version` text used as the drift pin.
    pub version: String,
    /// A headless `exec` subcommand is declared.
    pub headless_exec: bool,
    /// `exec --json` machine-readable JSONL events are declared.
    pub json_events: bool,
    /// `resume` and `exec --session-id` continuation hooks are declared.
    pub session_resume: bool,
    /// `exec --output-schema` final-answer shaping is declared.
    pub output_schema: bool,
    /// Provider-reported usage counters are advertised.
    pub usage_reporting: bool,
    /// A dedicated CLI-level cancel command is declared.
    pub cancel_command: bool,
}

impl MuseCliCapabilities {
    /// Parses observed `--version`, root `--help`, and `exec --help` text.
    ///
    /// # Errors
    ///
    /// Returns [`MuseSpecError::WrongProvider`] when the version text does
    /// not identify Muse Code.
    pub fn parse(version: &str, help: &str, exec_help: &str) -> Result<Self, MuseSpecError> {
        if !version.trim_start().starts_with("Muse Code") {
            return Err(MuseSpecError::WrongProvider);
        }
        Ok(Self {
            version: version.trim().to_owned(),
            headless_exec: help.contains("exec"),
            json_events: exec_help.contains("--json"),
            session_resume: help.contains("resume") && exec_help.contains("--session-id"),
            output_schema: exec_help.contains("--output-schema"),
            usage_reporting: exec_help.contains("--usage")
                || exec_help.contains("usage-report")
                || help.contains("--usage"),
            cancel_command: help.contains("\n  cancel ") || exec_help.contains("\n  cancel "),
        })
    }

    /// Minimum surface for attempting the typed adapter in 31.1.1.3:
    /// headless execution with JSONL events and a session-resume hook.
    #[must_use]
    pub const fn adapter_surface_ready(&self) -> bool {
        self.headless_exec && self.json_events && self.session_resume
    }

    /// Truthful blockers for required worker behavior the CLI does not
    /// declare. Usage observation and CLI-level cancellation stay open
    /// until adapter tests (31.1.1.3/31.1.1.5) or confinement proofs
    /// (31.1.1.4) demonstrate them another way.
    #[must_use]
    pub fn blockers(&self) -> Vec<&'static str> {
        let mut blockers = Vec::new();
        if !self.usage_reporting {
            blockers.push("usage counters are not advertised; observe_usage needs adapter proof");
        }
        if !self.cancel_command {
            blockers.push(
                "no CLI cancel command is advertised; cancellation needs process-level proof",
            );
        }
        blockers
    }
}

/// Checks reported capabilities against the Muse worker requirements.
///
/// # Errors
///
/// Returns [`MuseSpecError`] when the provider name, version, or any
/// required structured behavior is missing. Admission additionally
/// requires the exact CLI inspection and confinement proofs from the
/// later Story 31.1 sub-tasks.
pub fn admit(capabilities: &AgentCapabilities) -> Result<(), MuseSpecError> {
    if capabilities.provider != MUSE_PROVIDER {
        return Err(MuseSpecError::WrongProvider);
    }
    if capabilities.version.is_empty() {
        return Err(MuseSpecError::MissingVersion);
    }
    for required in required_capabilities() {
        if !capabilities.capabilities.contains(&required) {
            return Err(MuseSpecError::MissingCapability(required));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use codingmage_agent::{
        AdapterError, AgentAdapter, AgentEvent, AgentEventKind, AgentFinal, AgentFinalStatus,
        AgentOperation, AgentRequest, AgentRole, AgentUsage, FakeAdapter, FakeStep, ProviderClaims,
    };
    use codingmage_contracts::{AgentId, AttemptId, RunId, TaskId};

    use super::*;

    fn capabilities_without(missing: AgentCapability) -> AgentCapabilities {
        let mut required = required_capabilities();
        required.remove(&missing);
        AgentCapabilities {
            provider: MUSE_PROVIDER.to_owned(),
            version: String::from("fixture-cli-1"),
            capabilities: required,
        }
    }

    fn request(operation: AgentOperation, session_id: Option<AttemptId>) -> AgentRequest {
        AgentRequest {
            version: 1,
            run_id: RunId::new("run-1").expect("valid fixture"),
            task_id: TaskId::new("task-1").expect("valid fixture"),
            agent_id: AgentId::new("agent-muse").expect("valid fixture"),
            role: AgentRole::Implementation,
            operation,
            session_id,
            input_sha256: "b".repeat(64),
        }
    }

    fn completed_final() -> AgentFinal {
        AgentFinal {
            status: AgentFinalStatus::Completed,
            claims: ProviderClaims {
                commit: Some("b".repeat(40)),
                tests_passed: true,
                merge_completed: false,
                release_published: false,
            },
            blocker_code: None,
        }
    }

    fn transcript(session: &AttemptId, input_units: u64, output_units: u64) -> Vec<u8> {
        let events = [
            AgentEvent {
                version: 1,
                sequence: 0,
                session_id: session.clone(),
                event: AgentEventKind::Started,
            },
            AgentEvent {
                version: 1,
                sequence: 1,
                session_id: session.clone(),
                event: AgentEventKind::Usage {
                    input_units,
                    output_units,
                },
            },
            AgentEvent {
                version: 1,
                sequence: 2,
                session_id: session.clone(),
                event: AgentEventKind::Final {
                    result: completed_final(),
                },
            },
        ];
        events
            .iter()
            .map(|event| serde_json::to_string(event).expect("valid fixture"))
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes()
    }

    fn scripted_lifecycle() -> (FakeAdapter, AttemptId) {
        let session = AttemptId::new("attempt-1").expect("valid fixture");
        let adapter = FakeAdapter::new(
            MUSE_PROVIDER,
            vec![
                FakeStep {
                    operation: AgentOperation::Start,
                    response: Ok(transcript(&session, 10, 4)),
                },
                FakeStep {
                    operation: AgentOperation::Continue,
                    response: Ok(transcript(&session, 7, 3)),
                },
                FakeStep {
                    operation: AgentOperation::Cancel,
                    response: Ok(Vec::new()),
                },
            ],
        );
        (adapter, session)
    }

    #[test]
    fn admission_accepts_the_exact_required_behavior() {
        let adapter = FakeAdapter::new(MUSE_PROVIDER, Vec::new());
        let reported = adapter.probe().expect("valid fixture");
        assert_eq!(reported.provider, MUSE_PROVIDER);
        assert!(admit(&reported).is_ok());
    }

    #[test]
    fn admission_rejects_each_missing_behavior() {
        for missing in required_capabilities() {
            assert_eq!(
                admit(&capabilities_without(missing)),
                Err(MuseSpecError::MissingCapability(missing))
            );
        }
    }

    #[test]
    fn admission_rejects_wrong_provider_and_missing_version() {
        let adapter = FakeAdapter::new("other-provider", Vec::new());
        let reported = adapter.probe().expect("valid fixture");
        assert_eq!(admit(&reported), Err(MuseSpecError::WrongProvider));
        let unversioned = AgentCapabilities {
            provider: MUSE_PROVIDER.to_owned(),
            version: String::new(),
            capabilities: required_capabilities(),
        };
        assert_eq!(admit(&unversioned), Err(MuseSpecError::MissingVersion));
    }

    #[test]
    fn fake_transcripts_round_trip_probe_start_continue_usage_cancel() {
        let (mut adapter, session) = scripted_lifecycle();
        assert!(admit(&adapter.probe().expect("valid fixture")).is_ok());
        adapter
            .start(&request(AgentOperation::Start, None))
            .expect("valid fixture");
        adapter
            .continue_session(&request(AgentOperation::Continue, Some(session.clone())))
            .expect("valid fixture");
        assert_eq!(
            adapter.observe_usage(&request(
                AgentOperation::ObserveUsage,
                Some(session.clone())
            )),
            Ok(AgentUsage {
                input_units: 7,
                output_units: 3,
            })
        );
        adapter
            .cancel(&request(AgentOperation::Cancel, Some(session.clone())))
            .expect("valid fixture");
        assert_eq!(
            adapter.cancel(&request(AgentOperation::Cancel, Some(session))),
            Err(AdapterError::InvalidRequest)
        );
    }

    #[test]
    fn fake_transcripts_refuse_wrong_session_continuation() {
        let (mut adapter, session) = scripted_lifecycle();
        adapter
            .start(&request(AgentOperation::Start, None))
            .expect("valid fixture");
        let foreign = AttemptId::new("attempt-9").expect("valid fixture");
        assert_eq!(
            adapter.continue_session(&request(AgentOperation::Continue, Some(foreign))),
            Err(AdapterError::InvalidRequest)
        );
        adapter
            .continue_session(&request(AgentOperation::Continue, Some(session)))
            .expect("valid fixture");
    }

    /// Verbatim excerpts of the observed `--version`, root `--help`, and
    /// `exec --help` output. Paths are excluded; only the identity and
    /// protocol-declaration lines are pinned.
    fn observed_version() -> &'static str {
        "Muse Code 1.3.0 (1.3.0-R3401.1)\n"
    }

    fn observed_help() -> &'static str {
        "muse \u{2014} interactive terminal coding agent\n\nUsage: muse [OPTIONS] [PROMPT]\n       muse [OPTIONS] <COMMAND>\n\nCommands:\n  resume           Resume a previous session (--last or <session-ref>)\n  exec             Run one prompt non-interactively (headless)\n"
    }

    fn observed_exec_help() -> &'static str {
        "muse exec \u{2014} run one prompt non-interactively (headless)\n\nUsage: muse exec [OPTIONS] [PROMPT]\n\nOptions:\n      --json\n          Emit machine-readable JSONL events on stdout\n      --output-schema <FILE>\n          Shape the final answer with the JSON schema in FILE\n      --session-id <UUID>\n          Use a specific session id\n          Offer request_user_input and auto-cancel prompts (headless)\n"
    }

    #[test]
    fn observed_cli_identity_pins_exact_surface_and_blockers() {
        let observed =
            MuseCliCapabilities::parse(observed_version(), observed_help(), observed_exec_help())
                .expect("valid fixture");
        assert_eq!(observed.version, "Muse Code 1.3.0 (1.3.0-R3401.1)");
        assert!(observed.headless_exec);
        assert!(observed.json_events);
        assert!(observed.session_resume);
        assert!(observed.output_schema);
        assert!(!observed.usage_reporting);
        assert!(!observed.cancel_command);
        assert!(observed.adapter_surface_ready());
        assert_eq!(observed.blockers().len(), 2);
    }

    #[test]
    fn cli_parse_rejects_unrecognized_identity() {
        assert_eq!(
            MuseCliCapabilities::parse("other tool 1.0", observed_help(), observed_exec_help()),
            Err(MuseSpecError::WrongProvider)
        );
    }

    #[test]
    fn cli_parse_does_not_mistake_prose_for_a_cancel_command() {
        let observed =
            MuseCliCapabilities::parse(observed_version(), observed_help(), observed_exec_help())
                .expect("valid fixture");
        assert!(observed_exec_help().contains("auto-cancel"));
        assert!(!observed.cancel_command);
    }

    #[test]
    fn adapter_surface_requires_all_three_hooks() {
        let observed =
            MuseCliCapabilities::parse(observed_version(), observed_help(), observed_exec_help())
                .expect("valid fixture");
        assert!(observed.adapter_surface_ready());
        let no_json = MuseCliCapabilities {
            json_events: false,
            ..observed
        };
        assert!(!no_json.adapter_surface_ready());
    }

    #[test]
    fn spec_error_codes_are_stable() {
        assert_eq!(
            MuseSpecError::WrongProvider.to_string(),
            "codingmage.muse.wrong_provider"
        );
        assert_eq!(
            MuseSpecError::MissingVersion.to_string(),
            "codingmage.muse.missing_version"
        );
        assert_eq!(
            MuseSpecError::MissingCapability(AgentCapability::Cancellation).to_string(),
            "codingmage.muse.missing_capability.Cancellation"
        );
    }
}
