pub use exonum::runtime::ExecutionContext;

pub use self::{
    error::Error,
    runtime_api::{ArtifactProtobufSpec, ProtoSourceFile, ProtoSourcesQuery},
    service::{CounterService, WasmService},
};

use exonum::{
    blockchain::Blockchain,
    merkledb::{BinaryValue, Snapshot},
    runtime::{
        migrations::{InitMigrationError, MigrationScript},
        oneshot::Receiver,
        versioning::Version,
        ArtifactId, ExecutionError, ExecutionFail, InstanceDescriptor, InstanceId, InstanceSpec,
        InstanceState, InstanceStatus, Mailbox, MethodId, Runtime, RuntimeFeature,
        RuntimeIdentifier, WellKnownRuntime,
    },
};
use exonum_api::{ApiBuilder, UpdateEndpoints};
use futures::{channel::mpsc, executor, SinkExt};
use log::trace;
use wasmer_runtime::Func;

use std::collections::{BTreeMap, HashMap, HashSet};
use crate::vm::logic::ReturnData;

//use self::api::ServiceApiBuilder;

mod error;
mod runtime_api;
mod vm;
pub mod service;

#[doc(hidden)]
pub mod _reexports {
    //! Types necessary for `ServiceDispatcher` and `ServiceFactory` derive macros to work.

    pub use exonum::runtime::{
        ArtifactId, CommonError, ExecutionContext, ExecutionError, MethodId, RuntimeIdentifier,
    };
}

/// Wasm runtime entity.
#[derive(Debug)]
pub struct WasmRuntime {
    blockchain: Option<Blockchain>,
    api_notifier: mpsc::Sender<UpdateEndpoints>,
    available_artifacts: HashMap<ArtifactId, WasmService>,
    deployed_artifacts: HashSet<ArtifactId>,
    started_services: BTreeMap<InstanceId, Instance>,
    started_services_by_name: HashMap<String, InstanceId>,
    changed_services_since_last_block: bool,
}

/// Builder of the `WasmRuntime`
#[derive(Debug, Default)]
pub struct WasmRuntimeBuilder {
    available_artifacts: HashMap<ArtifactId, WasmService>,
}

#[derive(Debug)]
struct Instance {
    id: InstanceId,
    name: String,
    service: WasmService,
    artifact_id: ArtifactId,
}

impl Instance {
    fn descriptor(&self) -> InstanceDescriptor { InstanceDescriptor::new(self.id, &self.name) }
}

//impl AsRef<dyn Service> for Instance {
//    fn as_ref(&self) -> &dyn Service { self.service.as_ref()}
//}

impl WasmRuntimeBuilder {
    pub fn new() -> Self { Self::default() }

    pub fn with_factory(mut self, artifact_name: &str) -> Self {
        let version = Version::new(1,1, 1);
        let runtime_id = RuntimeIdentifier::Wasm as u32;
        self.available_artifacts
            .insert(ArtifactId::new(runtime_id, artifact_name.to_string(), version).unwrap(), WasmService::new("counter-service").unwrap());
        self
    }

    pub fn build(self, api_notifier: mpsc::Sender<UpdateEndpoints>) -> WasmRuntime {
        WasmRuntime {
            blockchain: None,
            api_notifier,
            available_artifacts: self.available_artifacts,
            deployed_artifacts: HashSet::new(),
            started_services: BTreeMap::new(),
            started_services_by_name: HashMap::new(),
            changed_services_since_last_block: true,
        }
    }

    pub fn build_for_tests(self) -> WasmRuntime{ self.build(mpsc::channel(1).0) }
}

impl WasmRuntime {
    pub const NAME: &'static str = "wasm";

    pub fn builder() -> WasmRuntimeBuilder{ WasmRuntimeBuilder::new() }

    fn assert_known_status(status: &InstanceStatus) {
        match status {
            InstanceStatus::Active
            | InstanceStatus::Stopped
            | InstanceStatus::Frozen
            | InstanceStatus::Migrating(_) => (),

            other => {
                panic!(
                    "Received non-expected service status: {}; \
                     Rust runtime isn't prepared to process this action, \
                     probably Rust runtime is outdated relative to the core library",
                    other
                );
            }
        }
    }

    fn blockchain(&self) -> &Blockchain {
        self.blockchain
            .as_ref()
            .expect("Method called before Rust runtime is initialized")
    }

    fn add_started_service(&mut self, instance: Instance) {
        self.started_services_by_name
            .insert(instance.name.clone(), instance.id);
        self.started_services.insert(instance.id, instance);
    }

    fn remove_started_service(&mut self, instance: &InstanceSpec) {
        self.started_services_by_name.remove(&instance.name);
        self.started_services.remove(&instance.id);
    }

    fn deploy(&mut self, artifact: &ArtifactId) -> Result<(), ExecutionError> {
        if self.deployed_artifacts.contains(artifact) {
            panic!(
                "BUG: Core requested deploy of already deployed artifact {:?}",
                artifact
            );
        }
        if !self.available_artifacts.contains_key(artifact) {
            let description = format!(
                "Runtime failed to deploy artifact with id {}, \
                 it is not listed among available artifacts. Available artifacts: {}",
                artifact,
                self.artifacts_to_pretty_string()
            );
            return Err(Error::UnableToDeploy.with_description(description));
        }
        trace!("Deployed artifact: {}", artifact);
        self.deployed_artifacts.insert(artifact.to_owned());
        Ok(())
    }

    fn new_service(
        &self,
        artifact: &ArtifactId,
        instance: &InstanceDescriptor,
    ) -> Result<Instance, ExecutionError> {
        let service = self.available_artifacts.get(artifact).unwrap_or_else(|| {
            panic!(
                "BUG: Core requested service instance start ({}) of not deployed artifact {}",
                instance.name, artifact
            );
        }).clone();

        Ok(Instance {
            id: instance.id,
            name: instance.name.to_owned(),
            service,
            artifact_id: artifact.to_owned(),
        })
    }

    fn new_service_if_needed(
        &self,
        artifact: &ArtifactId,
        descriptor: &InstanceDescriptor,
    ) -> Result<Option<Instance>, ExecutionError> {
        if let Some(instance) = self.started_services.get(&descriptor.id) {
            assert!(
                instance.artifact_id == *artifact || artifact.is_upgrade_of(&instance.artifact_id),
                "Mismatch between the requested artifact and the artifact associated \
                 with the running service {}. This is either a bug in the lifecycle \
                 workflow in the core, or this version of the Rust runtime is outdated \
                 compared to the core.",
                descriptor
            );

            if instance.artifact_id == *artifact {
                // We just continue running the existing service since we've just checked
                // that it corresponds to the same artifact.
                return Ok(None);
            }
        }
        Some(self.new_service(artifact, descriptor)).transpose()
    }

    fn artifacts_to_pretty_string(&self) -> String {
        if self.available_artifacts.is_empty() {
            return "None".to_string();
        }

        self.available_artifacts
            .keys()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
            .join(", ")
    }
}

impl WellKnownRuntime for WasmRuntime {
    const ID: u32 = RuntimeIdentifier::Wasm as u32;
}

impl Runtime for WasmRuntime {
    fn initialize(&mut self, blockchain: &Blockchain) {
        self.blockchain = Some(blockchain.clone())
    }

    fn is_supported(&self, feature: &RuntimeFeature) -> bool {
        match feature {
            _ => false
        }
    }

    fn deploy_artifact(&mut self, artifact: ArtifactId, spec: Vec<u8>) -> Receiver {
        let result = if spec.is_empty() {
            self.deploy(&artifact)
        } else {
            Err(Error::IncorrectArtifactId.into())
        };
        Receiver::with_result(result)
    }

    fn is_artifact_deployed(&self, id: &ArtifactId) -> bool {
        self.deployed_artifacts.contains(id)
    }

    // Unloading an artifact is effectively a no-op.
    fn unload_artifact(&mut self, artifact: &ArtifactId) {
        let was_present = self.deployed_artifacts.remove(artifact);
        debug_assert!(
            was_present,
            "Requested to unload non-existing artifact `{}`",
            artifact
        );
    }

    fn initiate_adding_service(
        &self,
        context: ExecutionContext<'_>,
        artifact: &ArtifactId,
        parameters: Vec<u8>,
    ) -> Result<(), ExecutionError> {
        println!(
            "Wasm service Initializing service artifact - {}: context.instance - {}",
            artifact,
            context.instance()
        );
        Ok(())
    }

    fn initiate_resuming_service(
        &self,
        context: ExecutionContext<'_>,
        artifact: &ArtifactId,
        parameters: Vec<u8>,
    ) -> Result<(), ExecutionError> {
        unreachable!("Don't support resume services in this version.")
    }

    fn update_service_status(&mut self, _snapshot: &dyn Snapshot, state: &InstanceState) {
        const CANNOT_INSTANTIATE_SERVICE: &str =
            "BUG: Attempt to create a new service instance failed; \
             within `instantiate_adding_service` we were able to create a new instance, \
             but now we are not.";

        let status = state
            .status
            .as_ref()
            .expect("Rust runtime does not support removing service status");
        Self::assert_known_status(status);

        let mut service_api_changed = false;
        let switch_off = if status.provides_read_access() {
            if let Some(artifact) = state.associated_artifact() {
                // Instantiate the service if necessary.
                let maybe_instance = self
                    .new_service_if_needed(artifact, &state.spec.as_descriptor())
                    .expect(CANNOT_INSTANTIATE_SERVICE);
                if let Some(instance) = maybe_instance {
                    self.add_started_service(instance);
                    // The service API has changed even if it was previously instantiated
                    // (in the latter case, the instantiated version is outdated).
                    service_api_changed = true;
                }
                false
            } else {
                // Service is no longer associated with an artifact; its API needs switching off.
                true
            }
        } else {
            // Service status (e.g., `Stopped`) requires switching the API off.
            true
        };
    }

    fn migrate(
        &self,
        new_artifact: &ArtifactId,
        data_version: &Version,
    ) -> Result<Option<MigrationScript>, InitMigrationError> {
        Err(InitMigrationError::NotSupported)
    }

    fn execute(
        &self,
        context: ExecutionContext<'_>,
        method_id: MethodId,
        payload: &[u8],
    ) -> Result<(), ExecutionError> {
        let instance = self
            .started_services
            .get(&context.instance().id)
            .ok_or(Error::IncorrectCallInfo)?;
        let service = &instance.service;

        println!(
            "Executing method {}#{} of service {}",
            context.interface_name(),
            method_id,
            context.instance().id
        );

        const SERVICE_INTERFACE: &str = "";
        match (context.interface_name(), method_id) {
            // Increment counter.
            (SERVICE_INTERFACE, 0) => {
                let value = u64::from_bytes(payload.into())
                    .map_err(|e| Error::UnknownTransaction.with_description(e))?;
                let instance = service.instantiate();
                let add_one: Func<u32, u32> = instance.func("add_one").unwrap();
                let result = add_one.call(value as u32).unwrap();
                println!("Updating counter value to {}", result);
                //service.counter.set(counter + value);
                Ok(())
            }

            // Reset counter.
            (SERVICE_INTERFACE, 1) => {
                println!("Resetting counter");
                let value = u64::from_bytes(payload.into())
                    .map_err(|e| Error::UnknownTransaction.with_description(e))?;
                //let instance = service.instantiate();
                //let add_one: Func<(u32, u32), u32> = instance.func("increment_counter").unwrap();
                //let result = add_one.call(value as u32, 10 as u32).unwrap();
                let result = service.run(b"ext_read_vec").0;
                match result {
                    Some(v) => {
                        if let ReturnData::Value(value) = v {
                            //println!("Host function exposed result {}", value);
                            dbg!(value);
                        }
                    },
                    None => {
                        println!("Service panicked")
                    },
                };

                Ok(())
            }

            // Unknown transaction.
            (interface, method) => {
                let err = Error::UnknownTransaction.with_description(format!(
                    "Incorrect information to call transaction. {}#{}",
                    interface, method
                ));
                Err(err)
            }
        }
    }

    fn before_transactions(&self, context: ExecutionContext<'_>) -> Result<(), ExecutionError> {
        Ok(())
    }

    fn after_transactions(&self, context: ExecutionContext<'_>) -> Result<(), ExecutionError> {
        Ok(())
    }

    fn after_commit(&mut self, snapshot: &dyn Snapshot, mailbox: &mut Mailbox) { }

}
