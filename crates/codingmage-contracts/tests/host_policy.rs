//! Policy-subjection tripwires for the host contract.
//!
//! Local preparation only: these tests prove that every host operation stays
//! inside the observation/control plane and can never express approval,
//! promotion, publication, policy change, or credential use. The tripwires
//! are exhaustive matches plus variant-count pins: adding any operation
//! breaks compilation or the count assertion first, forcing an explicit
//! policy decision before the new operation can exist.

use codingmage_contracts::{HOST_PROTOCOL_VERSION, HostControlOperation, HostOperation};

/// Every job-plane operation the contract will ever admit, pinned by count.
const ALL_OPERATIONS: [HostOperation; 7] = [
    HostOperation::SubmitJob,
    HostOperation::ReportStatus,
    HostOperation::ObserveEvents,
    HostOperation::Pause,
    HostOperation::Resume,
    HostOperation::StopAfterUnit,
    HostOperation::Cancel,
];

/// Every control-plane operation, pinned by count.
const ALL_CONTROLS: [HostControlOperation; 4] = [
    HostControlOperation::Pause,
    HostControlOperation::Resume,
    HostControlOperation::StopAfterUnit,
    HostControlOperation::Cancel,
];

/// Classifies one operation as control-plane-only. There is no other arm by
/// construction: approval, promotion, publication, policy, and credential
/// effects have no representation in either host enum.
fn classify(operation: HostOperation) -> &'static str {
    match operation {
        HostOperation::SubmitJob => "bounded-job",
        HostOperation::ReportStatus | HostOperation::ObserveEvents => "observation",
        HostOperation::Pause
        | HostOperation::Resume
        | HostOperation::StopAfterUnit
        | HostOperation::Cancel => "control",
    }
}

#[test]
fn every_operation_is_pinned_and_control_plane_only() {
    assert_eq!(ALL_OPERATIONS.len(), 7);
    for operation in ALL_OPERATIONS {
        let class = classify(operation);
        assert!(
            matches!(class, "bounded-job" | "observation" | "control"),
            "{operation:?}"
        );
    }
}

#[test]
fn control_subset_maps_exactly_to_job_operations() {
    assert_eq!(ALL_CONTROLS.len(), 4);
    for control in ALL_CONTROLS {
        let operation = match control {
            HostControlOperation::Pause => HostOperation::Pause,
            HostControlOperation::Resume => HostOperation::Resume,
            HostControlOperation::StopAfterUnit => HostOperation::StopAfterUnit,
            HostControlOperation::Cancel => HostOperation::Cancel,
        };
        assert!(ALL_OPERATIONS.contains(&operation));
        assert_ne!(operation, HostOperation::SubmitJob);
    }
}

#[test]
fn observation_operations_carry_no_control() {
    for operation in [
        HostOperation::SubmitJob,
        HostOperation::ReportStatus,
        HostOperation::ObserveEvents,
    ] {
        let as_control = match operation {
            HostOperation::Pause => Some(HostControlOperation::Pause),
            HostOperation::Resume => Some(HostControlOperation::Resume),
            HostOperation::StopAfterUnit => Some(HostControlOperation::StopAfterUnit),
            HostOperation::Cancel => Some(HostControlOperation::Cancel),
            HostOperation::SubmitJob
            | HostOperation::ReportStatus
            | HostOperation::ObserveEvents => None,
        };
        assert_eq!(as_control, None, "{operation:?}");
    }
}

#[test]
fn contract_pins_protocol_version_one() {
    assert_eq!(HOST_PROTOCOL_VERSION, 1);
}
