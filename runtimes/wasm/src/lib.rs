pub use exonum::runtime::ExecutionContext;

pub use self::{
    error::Error,
    runtime_api::{ArtifactProtobufSpec, ProtoSourceFile, ProtoSourcesQuery},
    service::{
        AfterCommitContext, Broadcaster, DefaultInstance, Service, ServiceDispatcher,
        ServiceFactory,
    },
};

use exonum::{
    blockchain::{Blockchain, Schema as CoreSchema},
    helpers::Height,
    merkledb::Snapshot,
    runtime::{
        catch_panic,
        migrations::{InitMigrationError, MigrateData, MigrationScript},
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

use std::collections::{BTreeMap, HashMap, HashSet};

//use self::api::ServiceApiBuilder;

mod error;
mod runtime_api;
mod service;

#[doc(hidden)]
pub mod _reexports {
    //! Types necessary for `ServiceDispatcher` and `ServiceFactory` derive macros to work.

    pub use exonum::runtime::{
        ArtifactId, CommonError, ExecutionContext, ExecutionError, MethodId, RuntimeIdentifier,
    };
}

/// Wrapper around a service factory that does not support migrations.
#[derive(Debug)]
struct WithoutMigrations<T>(T);

impl<T: ServiceFactory> ServiceFactory for WithoutMigrations<T> {
    fn artifact_id(&self) -> ArtifactId {
        self.0.artifact_id()
    }

    fn artifact_protobuf_spec(&self) -> ArtifactProtobufSpec {
        self.0.artifact_protobuf_spec()
    }

    fn create_instance(&self) -> Box<dyn Service> {
        self.0.create_instance()
    }
}

impl<T> MigrateData for WithoutMigrations<T> {
    fn migration_scripts(
        &self,
        _start_version: &Version,
    ) -> Result<Vec<MigrationScript>, InitMigrationError> {
        Err(InitMigrationError::NotSupported)
    }
}

/// Wasm runtime entity.
#[derive(Debug)]
pub struct WasmRuntime {
    blockchain: Option<Blockchain>,
    api_notifier: mpsc::Sender<UpdateEndpoints>,
    available_artifacts: HashMap<ArtifactId, Box<dyn ServiceFactory>>,
    deployed_artifacts: HashSet<ArtifactId>,
    started_services: BTreeMap<InstanceId, Instance>,
    started_services_by_name: HashMap<String, InstanceId>,
    changed_services_since_last_block: bool,
}

/// Builder of the `WasmRuntime`
#[derive(Debug, Default)]
pub struct WasmRuntimeBuilder {
    available_artifacts: HashMap<ArtifactId, Box<dyn ServiceFactory>>,
}

#[derive(Debug)]
struct Instance {
    id: InstanceId,
    name: String,
    service: Box<dyn Service>,
    artifact_id: ArtifactId,
}

impl Instance {
    fn descriptor(&self) -> InstanceDescriptor { InstanceDescriptor::new(self.id, &self.name) }
}

impl AsRef<dyn Service> for Instance {
    fn as_ref(&self) -> &dyn Service { self.service.as_ref()}
}

impl WasmRuntimeBuilder {
    pub fn new() -> Self { Self::default() }

    pub fn with_factory<S: ServiceFactory>(mut self, service_factory: S) -> Self {
        let artifact = service_factory.artifact_id();
        let service_factory = WithoutMigrations(service_factory);
        self.available_artifacts
            .insert(artifact, Box::new(service_factory));
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
    pub const NAME: &'static str = "rust";

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
        let factory = self.available_artifacts.get(artifact).unwrap_or_else(|| {
            panic!(
                "BUG: Core requested service instance start ({}) of not deployed artifact {}",
                instance.name, artifact
            );
        });

        let service = factory.create_instance();
        Ok(Instance {
            id: instance.id,
            name: instance.name.to_owned(),
            service,
            artifact_id: artifact.to_owned(),
        })
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
        let instance = self.new_service(artifact, context.instance())?;
        let service = instance.as_ref();
        catch_panic(|| service.initialize(context, parameters))
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
        unimplemented!()
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
            .expect("BUG: an attempt to execute transaction of unknown service.");
        catch_panic(|| instance.as_ref().call(context, method_id, payload))
    }

    fn before_transactions(&self, context: ExecutionContext<'_>) -> Result<(), ExecutionError> {
        Ok(())
    }

    fn after_transactions(&self, context: ExecutionContext<'_>) -> Result<(), ExecutionError> {
        Ok(())
    }

    fn after_commit(&mut self, snapshot: &dyn Snapshot, mailbox: &mut Mailbox) { }

}
