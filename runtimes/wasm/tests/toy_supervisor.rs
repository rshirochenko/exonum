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

use exonum_rust_runtime::{DefaultInstance, RustRuntime, Service, ServiceFactory};

#[exonum_interface(auto_ids)]
pub trait ToySupervisor<Ctx> {
    type Output;

    fn deploy_artifact(&self, context: Ctx, request: DeployArtifact) -> Self::Output;
    fn unload_artifact(&self, context: Ctx, artifact: ArtifactId) -> Self::Output;
    fn start_service(&self, context: Ctx, request: StartService) -> Self::Output;
    fn stop_service(&self, context: Ctx, instance_id: InstanceId) -> Self::Output;
    fn freeze_service(&self, context: Ctx, instance_id: InstanceId) -> Self::Output;
    fn resume_service(&self, context: Ctx, request: ResumeService) -> Self::Output;
    fn migrate_service(&self, context: Ctx, request: MigrateService) -> Self::Output;
    fn commit_migration(&self, context: Ctx, request: CommitMigration) -> Self::Output;
    fn flush_migration(&self, context: Ctx, instance_name: String) -> Self::Output;
}

#[derive(Debug, ServiceFactory, ServiceDispatcher)]
#[service_dispatcher(implements("ToySupervisor"))]
#[service_factory(artifact_name = "toy_supervisor", artifact_version = "0.1.0")]
pub struct ToySupervisorService;

impl ToySupervisor<ExecutionContext<'_>> for ToySupervisorService {
    type Output = Result<(), ExecutionError>;

    fn deploy_artifact(
        &self,
        mut context: ExecutionContext<'_>,
        request: DeployArtifact,
    ) -> Self::Output {
        context
            .supervisor_extensions()
            .start_artifact_registration(&request.test_service_artifact, request.spec);
        Ok(())
    }

    fn unload_artifact(
        &self,
        mut context: ExecutionContext<'_>,
        artifact: ArtifactId,
    ) -> Self::Output {
        context.supervisor_extensions().unload_artifact(&artifact)
    }

    fn start_service(
        &self,
        mut context: ExecutionContext<'_>,
        request: StartService,
    ) -> Self::Output {
        context
            .supervisor_extensions()
            .initiate_adding_service(request.spec, request.constructor)
    }

    fn stop_service(
        &self,
        mut context: ExecutionContext<'_>,
        instance_id: InstanceId,
    ) -> Self::Output {
        context
            .supervisor_extensions()
            .initiate_stopping_service(instance_id)
    }

    fn freeze_service(
        &self,
        mut context: ExecutionContext<'_>,
        instance_id: InstanceId,
    ) -> Self::Output {
        context
            .supervisor_extensions()
            .initiate_freezing_service(instance_id)
    }

    fn resume_service(
        &self,
        mut context: ExecutionContext<'_>,
        request: ResumeService,
    ) -> Self::Output {
        context
            .supervisor_extensions()
            .initiate_resuming_service(request.instance_id, request.params)
    }

    fn migrate_service(
        &self,
        mut context: ExecutionContext<'_>,
        request: MigrateService,
    ) -> Self::Output {
        context
            .supervisor_extensions()
            .initiate_migration(request.artifact, &request.instance_name)
            .map(drop)
    }

    fn commit_migration(
        &self,
        mut context: ExecutionContext<'_>,
        request: CommitMigration,
    ) -> Self::Output {
        context
            .supervisor_extensions()
            .commit_migration(&request.instance_name, request.migration_hash)
    }

    fn flush_migration(
        &self,
        mut context: ExecutionContext<'_>,
        instance_name: String,
    ) -> Self::Output {
        context
            .supervisor_extensions()
            .flush_migration(&instance_name)
    }
}

impl Service for ToySupervisorService {}

impl DefaultInstance for ToySupervisorService {
    const INSTANCE_ID: u32 = SUPERVISOR_INSTANCE_ID;
    const INSTANCE_NAME: &'static str = "supervisor";

    fn default_instance(&self) -> InstanceInitParams {
        self.artifact_id()
            .into_default_instance(Self::INSTANCE_ID, Self::INSTANCE_NAME)
    }
}
