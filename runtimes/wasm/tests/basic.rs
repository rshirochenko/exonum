use exonum::{
    blockchain::{
        config::{GenesisConfig, InstanceInitParams},
        Blockchain, BlockchainBuilder, BlockchainMut,
    },
    helpers::Height,
    merkledb::{access::AccessExt, BinaryValue, SystemSchema},
    runtime::{
        Caller, CommonError, CoreError, ErrorMatch, ExecutionContext, ExecutionError,
        InstanceStatus, SnapshotExt,
    },
};
use exonum_wasm_runtime::{DefaultInstance, WasmRuntimeBuilder, Service, ServiceFactory};
use exonum_derive::{exonum_interface, BinaryValue, ServiceDispatcher, ServiceFactory};
use pretty_assertions::assert_eq;
use serde_derive::{Deserialize, Serialize};

use self::inspected::{
    create_block_with_transactions, create_genesis_config_builder, execute_transaction,
    EventsHandle, Inspected, RuntimeEvent
};

pub mod inspected;

fn create_genesis_config_with_supervisor() -> GenesisConfig {
    create_genesis_config_builder()
        //.with_artifact(ToySupervisorService.artifact_id())
        //.with_instance(ToySupervisorService.default_instance())
        .build()
}

fn create_runtime(
    blockchain: Blockchain,
    genesis_config: GenesisConfig,
) -> (BlockchainMut, EventsHandle) {
    let inspected = Inspected::new(
        WasmRuntimeBuilder::new()
            //.with_factory(TestServiceImpl)
            .build_for_tests(),
    );
    let events_handle = inspected.events.clone();

    let blockchain = BlockchainBuilder::new(blockchain)
        .with_genesis_config(genesis_config)
        .with_runtime(inspected)
        .build();
    (blockchain, events_handle)
}

/// In this test, we manually instruct the dispatcher to deploy artifacts / create / stop services
/// instead of using transactions. We still need to create patches using a `BlockchainMut`
/// in order to properly emulate the blockchain workflow.
#[test]
fn basic_runtime_workflow() {
    // Create a runtime and a service test_service_artifact.
    let (mut blockchain, events_handle) = create_runtime(
        Blockchain::build_for_tests(),
        create_genesis_config_with_supervisor(),
    );
    let keypair = blockchain.as_ref().service_keypair().clone();
}
