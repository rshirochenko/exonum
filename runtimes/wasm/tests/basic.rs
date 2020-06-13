use exonum::{
    blockchain::{
        config::{GenesisConfig, InstanceInitParams},
        Blockchain, BlockchainBuilder, BlockchainMut,
    },
    helpers::Height,
    merkledb::{access::AccessExt, BinaryValue, SystemSchema},
    runtime::{
        Caller, CommonError, CoreError, ErrorMatch, ExecutionContext, ExecutionError,
        InstanceStatus, SnapshotExt, AnyTx, CallInfo
    },
};
use exonum_wasm_runtime::{
    service::{CounterService,CounterServiceImpl},
    WasmRuntimeBuilder,
};
use exonum_wasm_runtime::service::DefaultInstanceService;
use exonum_derive::{exonum_interface, BinaryValue, ServiceDispatcher, ServiceFactory};
use exonum_rust_runtime::{RustRuntime, RustRuntimeBuilder};
use exonum_rust_runtime::service::{ServiceFactory, DefaultInstance};

use pretty_assertions::assert_eq;
use serde_derive::{Deserialize, Serialize};

use self::toy_supervisor::ToySupervisorService;
use self::inspected::{
    create_block_with_transactions, create_genesis_config_builder, execute_transaction,
    EventsHandle, Inspected, RuntimeEvent
};
use crate::toy_supervisor::ToySupervisor;
use crate::toy_supervisor::{DeployArtifact, StartService};

pub mod toy_supervisor;
pub mod inspected;

fn create_genesis_config_with_supervisor() -> GenesisConfig {
    create_genesis_config_builder()
        .with_artifact(ToySupervisorService.artifact_id())
        .with_instance(ToySupervisorService.default_instance())
        .build()
}

fn create_runtime(
    blockchain: Blockchain,
    genesis_config: GenesisConfig,
) -> (BlockchainMut, EventsHandle, EventsHandle) {
    let inspected_wasm = Inspected::new(
        WasmRuntimeBuilder::new()
            //.with_factory("test_service")
            .with_factory("test_service")
            .build_for_tests(),
    );
    let inspected_rust = Inspected::new(
        RustRuntimeBuilder::new()
            .with_factory(ToySupervisorService)
            .build_for_tests(),
    );

    let events_handle_rust = inspected_rust.events.clone();
    let events_handle_wasm = inspected_wasm.events.clone();

    let blockchain = BlockchainBuilder::new(blockchain)
        .with_genesis_config(genesis_config)
        .with_runtime(inspected_rust)
        .with_runtime(inspected_wasm)
        .build();
    (blockchain, events_handle_rust, events_handle_wasm)
}

/// In this test, we manually instruct the dispatcher to deploy artifacts / create / stop services
/// instead of using transactions. We still need to create patches using a `BlockchainMut`
/// in order to properly emulate the blockchain workflow.
#[test]
fn basic_runtime_workflow() {
    // Create a runtime and a service test_service_artifact.
    let (mut blockchain, events_handle_rust, events_handle_wasm) = create_runtime(
        Blockchain::build_for_tests(),
        create_genesis_config_with_supervisor(),
    );
    let keypair = blockchain.as_ref().service_keypair().clone();

    // The dispatcher should initialize the runtime and call `after_commit` for
    // the genesis block.
    let supervisor = ToySupervisorService.default_instance();
    assert_eq!(
        events_handle_rust.take(),
        vec![
            RuntimeEvent::InitializeRuntime,
            RuntimeEvent::DeployArtifact(ToySupervisorService.artifact_id(), vec![]),
            RuntimeEvent::StartAddingService(
                supervisor.instance_spec.clone(),
                supervisor.constructor
            ),
            RuntimeEvent::CommitService(
                Height(0),
                supervisor.instance_spec.clone(),
                InstanceStatus::Active,
            ),
            RuntimeEvent::AfterTransactions(Height(0), ToySupervisorService::INSTANCE_ID),
            RuntimeEvent::AfterCommit(Height(1)),
        ]
    );
    assert_eq!(
        events_handle_wasm.take(),
        vec![
            RuntimeEvent::InitializeRuntime,
            RuntimeEvent::AfterCommit(Height(1)),
        ]
    );

    // Deploy service test_service_artifact.
    let test_service_artifact = CounterServiceImpl.artifact_id();
    execute_transaction(
        &mut blockchain,
        keypair.deploy_artifact(
            ToySupervisorService::INSTANCE_ID,
            DeployArtifact {
                test_service_artifact: test_service_artifact.clone(),
                spec: vec![],
            },
        ),
    ).unwrap();

    assert_eq!(
        events_handle_rust.take(),
        vec![
            RuntimeEvent::BeforeTransactions(Height(1), ToySupervisorService::INSTANCE_ID),
            RuntimeEvent::AfterTransactions(Height(1), ToySupervisorService::INSTANCE_ID),
            RuntimeEvent::AfterCommit(Height(2)),
        ]
    );
    assert_eq!(
        events_handle_wasm.take(),
        vec![
            RuntimeEvent::DeployArtifact(test_service_artifact, vec![]),
            RuntimeEvent::AfterCommit(Height(2)),
        ]
    );

    // Add service instance.
    let test_instance = CounterServiceImpl.default_instance();
    execute_transaction(
        &mut blockchain,
        keypair.start_service(
            ToySupervisorService::INSTANCE_ID,
            StartService {
                spec: test_instance.instance_spec.clone(),
                constructor: test_instance.constructor.clone(),
            },
        ),
    ).unwrap();

    events_handle_rust.take();
    assert_eq!(
        events_handle_wasm.take(),
        // The service is not active at the beginning of the block, so `after_transactions`
        // and `before_transactions` should not be called for it.
        vec![
            RuntimeEvent::StartAddingService(
                test_instance.instance_spec.clone(),
                test_instance.constructor
            ),
            RuntimeEvent::CommitService(
                Height(3),
                test_instance.instance_spec.clone(),
                InstanceStatus::Active
            ),
            RuntimeEvent::AfterCommit(Height(3)),
        ]
    );

    // Execute transaction method increment counter .
    let method_id = 0;
    let tx = AnyTx::new(CallInfo::new(test_instance.instance_spec.id, method_id), 1_000_u64.into_bytes());
    execute_transaction(
        &mut blockchain,
        tx.sign_with_keypair(&keypair),
    ).unwrap();
    // Check usual events from runtime.
    events_handle_rust.take();
    assert_eq!(
        events_handle_wasm.take(),
        vec![
            RuntimeEvent::BeforeTransactions(Height(3), CounterServiceImpl::INSTANCE_ID),
            RuntimeEvent::AfterTransactions(Height(3), CounterServiceImpl::INSTANCE_ID),
            RuntimeEvent::AfterCommit(Height(4)),
        ]
    );

    // Execute transaction with reset counter .
    let method_id = 1;
    let tx = AnyTx::new(CallInfo::new(test_instance.instance_spec.id, method_id), 10_u64.into_bytes());
    execute_transaction(
        &mut blockchain,
        tx.sign_with_keypair(&keypair),
    ).unwrap();
    // Check usual events from runtime.
    events_handle_rust.take();
    assert_eq!(
        events_handle_wasm.take(),
        vec![
            RuntimeEvent::BeforeTransactions(Height(4), CounterServiceImpl::INSTANCE_ID),
            RuntimeEvent::AfterTransactions(Height(4), CounterServiceImpl::INSTANCE_ID),
            RuntimeEvent::AfterCommit(Height(5)),
        ]
    );

    // Stop service instance.
    execute_transaction(
        &mut blockchain,
        keypair.stop_service(
            ToySupervisorService::INSTANCE_ID,
            CounterServiceImpl::INSTANCE_ID,
        ),
    ).unwrap();
    assert_eq!(
        events_handle_rust.take(),
        vec![
            RuntimeEvent::BeforeTransactions(Height(5), ToySupervisorService::INSTANCE_ID),
            RuntimeEvent::AfterTransactions(Height(5), ToySupervisorService::INSTANCE_ID),
            RuntimeEvent::AfterCommit(Height(6)),
        ]
    );
    assert_eq!(
        events_handle_wasm.take(),
        vec![
            RuntimeEvent::BeforeTransactions(Height(5), CounterServiceImpl::INSTANCE_ID),
            RuntimeEvent::AfterTransactions(Height(5), CounterServiceImpl::INSTANCE_ID),
            RuntimeEvent::CommitService(
                Height(6),
                test_instance.instance_spec.clone(),
                InstanceStatus::Stopped,
            ),
            RuntimeEvent::AfterCommit(Height(6)),
        ]
    );
}
