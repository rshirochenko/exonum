// Copyright 2020 The Exonum Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A collection of helpers for testing Rust runtime.
use exonum::{
    blockchain::{
        config::{GenesisConfigBuilder, InstanceInitParams},
        BlockParams, BlockPatch, Blockchain, BlockchainMut, ConsensusConfig, Schema as CoreSchema,
    },
    crypto::Hash,
    helpers::{Height, ValidatorId},
    merkledb::{ObjectHash, Snapshot},
    messages::{AnyTx, Verified},
    runtime::{
        migrations::{InitMigrationError, MigrationScript},
        oneshot,
        versioning::Version,
        ArtifactId, ExecutionContext, ExecutionError, InstanceId, InstanceSpec, InstanceState,
        InstanceStatus, Mailbox, MethodId, Runtime, RuntimeFeature, SnapshotExt, WellKnownRuntime,
        SUPERVISOR_INSTANCE_ID,
    },
};
use exonum_api::UpdateEndpoints;
use exonum_derive::{exonum_interface, BinaryValue, ServiceDispatcher, ServiceFactory};
use futures::{channel::mpsc, FutureExt, StreamExt};
use serde_derive::*;

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use exonum_wasm_runtime::{WasmRuntime};
use exonum_rust_runtime::RustRuntime;

pub fn execute_transaction(
    blockchain: &mut BlockchainMut,
    tx: Verified<AnyTx>,
) -> Result<(), ExecutionError> {
    let tx_hash = tx.object_hash();

    let patch = create_block_with_transactions(blockchain, vec![tx]);
    blockchain.commit(patch, vec![]).unwrap();

    let snapshot = blockchain.snapshot();
    let schema = CoreSchema::new(&snapshot);
    let location = schema.transactions_locations().get(&tx_hash).unwrap();
    schema.transaction_result(location).unwrap()
}

pub fn create_block_with_transactions(
    blockchain: &mut BlockchainMut,
    transactions: Vec<Verified<AnyTx>>,
) -> BlockPatch {
    let tx_hashes = add_transactions_into_pool(blockchain, transactions);
    let block_params = BlockParams::new(ValidatorId(0), Height(100), &tx_hashes);
    blockchain.create_patch(block_params, &())
}

pub fn create_genesis_config_builder() -> GenesisConfigBuilder {
    let (consensus_config, _) = ConsensusConfig::for_tests(1);
    GenesisConfigBuilder::with_consensus_config(consensus_config)
}

fn add_transactions_into_pool(
    blockchain: &mut BlockchainMut,
    txs: Vec<Verified<AnyTx>>,
) -> Vec<Hash> {
    blockchain
        .merge({
            let fork = blockchain.fork();
            let mut schema = CoreSchema::new(&fork);
            for tx in txs.clone() {
                schema.add_transaction_into_pool(tx);
            }
            fork.into_patch()
        })
        .unwrap();

    txs.into_iter().map(|x| x.object_hash()).collect()
}

pub fn get_endpoint_paths(endpoints_rx: &mut mpsc::Receiver<UpdateEndpoints>) -> HashSet<String> {
    let received = endpoints_rx
        .next()
        .now_or_never()
        .expect("No endpoint update")
        .expect("Node sender was dropped");
    received.updated_paths().map(ToOwned::to_owned).collect()
}

pub fn assert_no_endpoint_update(endpoints_rx: &mut mpsc::Receiver<UpdateEndpoints>) {
    let maybe_update = endpoints_rx.next().now_or_never().flatten();
    if let Some(update) = maybe_update {
        panic!(
            "Unexpected endpoints update: {:?}",
            update.updated_paths().collect::<Vec<_>>()
        );
    }
}

#[derive(Debug, PartialEq)]
pub enum RuntimeEvent {
    InitializeRuntime,
    ResumeRuntime,
    BeforeTransactions(Height, InstanceId),
    DeployArtifact(ArtifactId, Vec<u8>),
    UnloadArtifact(ArtifactId),
    StartAddingService(InstanceSpec, Vec<u8>),
    MigrateService(ArtifactId, Version),
    StartResumingService(InstanceSpec, Vec<u8>),
    CommitService(Height, InstanceSpec, InstanceStatus),
    AfterTransactions(Height, InstanceId),
    AfterCommit(Height),
}

#[derive(Debug, Clone, Default)]
pub struct EventsHandle(Arc<Mutex<Vec<RuntimeEvent>>>);

impl EventsHandle {
    pub fn push(&self, event: RuntimeEvent) {
        self.0.lock().unwrap().push(event);
    }

    pub fn take(&self) -> Vec<RuntimeEvent> {
        self.0.lock().unwrap().drain(..).collect()
    }
}

/// Test runtime wrapper logging all the events (as `RuntimeEvent`) happening within it.
/// For service hooks the logged height is the height of the block **being processed**.
/// Other than logging, it just redirects all the calls to the inner runtime.
/// Used to test that workflow invariants are respected.
#[derive(Debug)]
pub struct Inspected<T> {
    runtime: T,
    pub events: EventsHandle,
}

impl<T: Runtime> Inspected<T> {
    pub fn new(runtime: T) -> Self {
        Self {
            runtime,
            events: Default::default(),
        }
    }
}

impl<T: Runtime> Runtime for Inspected<T> {
    fn initialize(&mut self, blockchain: &Blockchain) {
        self.events.push(RuntimeEvent::InitializeRuntime);
        self.runtime.initialize(blockchain)
    }

    fn is_supported(&self, feature: &RuntimeFeature) -> bool {
        self.runtime.is_supported(feature)
    }

    fn on_resume(&mut self) {
        self.events.push(RuntimeEvent::ResumeRuntime);
        self.runtime.on_resume()
    }

    fn deploy_artifact(
        &mut self,
        test_service_artifact: ArtifactId,
        deploy_spec: Vec<u8>,
    ) -> oneshot::Receiver {
        //dbg!("I am here2 {}", &self.runtime);
        self.events.push(RuntimeEvent::DeployArtifact(
            test_service_artifact.clone(),
            deploy_spec.clone(),
        ));
        //dbg!("events {}", &self.events);
        self.runtime
            .deploy_artifact(test_service_artifact, deploy_spec)
    }

    fn is_artifact_deployed(&self, id: &ArtifactId) -> bool {
        self.runtime.is_artifact_deployed(id)
    }

    fn unload_artifact(&mut self, artifact: &ArtifactId) {
        self.events
            .push(RuntimeEvent::UnloadArtifact(artifact.to_owned()));
        self.runtime.unload_artifact(artifact);
    }

    fn initiate_adding_service(
        &self,
        context: ExecutionContext<'_>,
        artifact: &ArtifactId,
        parameters: Vec<u8>,
    ) -> Result<(), ExecutionError> {
        let instance = context.instance();
        self.events.push(RuntimeEvent::StartAddingService(
            InstanceSpec::from_raw_parts(instance.id, instance.name.to_owned(), artifact.clone()),
            parameters.clone(),
        ));

        self.runtime
            .initiate_adding_service(context, artifact, parameters)
    }

    fn initiate_resuming_service(
        &self,
        context: ExecutionContext<'_>,
        artifact: &ArtifactId,
        parameters: Vec<u8>,
    ) -> Result<(), ExecutionError> {
        let instance = context.instance();
        self.events.push(RuntimeEvent::StartResumingService(
            InstanceSpec::from_raw_parts(instance.id, instance.name.to_owned(), artifact.clone()),
            parameters.clone(),
        ));

        self.runtime
            .initiate_resuming_service(context, artifact, parameters)
    }

    fn update_service_status(&mut self, snapshot: &dyn Snapshot, state: &InstanceState) {
        snapshot
            .for_dispatcher()
            .get_instance(state.spec.id)
            .expect("Service instance should exist");

        let core_schema = CoreSchema::new(snapshot);
        let height = core_schema.next_height();

        self.events.push(RuntimeEvent::CommitService(
            height,
            state.spec.to_owned(),
            state.status.to_owned().unwrap(),
        ));
        self.runtime.update_service_status(snapshot, state)
    }

    fn migrate(
        &self,
        new_artifact: &ArtifactId,
        data_version: &Version,
    ) -> Result<Option<MigrationScript>, InitMigrationError> {
        self.events.push(RuntimeEvent::MigrateService(
            new_artifact.to_owned(),
            data_version.clone(),
        ));
        self.runtime.migrate(new_artifact, data_version)
    }

    fn execute(
        &self,
        context: ExecutionContext<'_>,
        method_id: MethodId,
        arguments: &[u8],
    ) -> Result<(), ExecutionError> {
        self.runtime.execute(context, method_id, arguments)
    }

    fn before_transactions(&self, context: ExecutionContext<'_>) -> Result<(), ExecutionError> {
        let height = context.data().for_core().next_height();
        self.events.push(RuntimeEvent::BeforeTransactions(
            height,
            context.instance().id,
        ));
        self.runtime.after_transactions(context)
    }

    fn after_transactions(&self, context: ExecutionContext<'_>) -> Result<(), ExecutionError> {
        let schema = context.data().for_core();
        let height = schema.next_height();
        self.events.push(RuntimeEvent::AfterTransactions(
            height,
            context.instance().id,
        ));
        self.runtime.after_transactions(context)
    }

    fn after_commit(&mut self, snapshot: &dyn Snapshot, mailbox: &mut Mailbox) {
        let height = CoreSchema::new(snapshot).next_height();
        self.events.push(RuntimeEvent::AfterCommit(height));
        self.runtime.after_commit(snapshot, mailbox);
    }
}

impl WellKnownRuntime for Inspected<WasmRuntime> {
    const ID: u32 = WasmRuntime::ID;
}

impl WellKnownRuntime for Inspected<RustRuntime> {
    const ID: u32 = RustRuntime::ID;
}
