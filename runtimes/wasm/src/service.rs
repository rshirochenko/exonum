use std::{cell::Cell};

use exonum::blockchain::config::InstanceInitParams;
use exonum::{
    runtime::{versioning::Version, ArtifactId, InstanceId, RuntimeIdentifier},
};
/// Service instance with a counter.
#[derive(Debug, Default, Clone)]
pub struct CounterService {
    pub counter: Cell<u64>,
    pub name: String,
}

pub struct CounterServiceImpl;

pub trait DefaultInstanceService {
    const INSTANCE_ID: u32;
    /// Default name for a service.
    const INSTANCE_NAME: &'static str;

    /// Creates default instance configuration parameters for the service.
    fn default_instance(&self) -> InstanceInitParams;

    /// Return artifact id
    fn artifact_id(&self) -> ArtifactId;
}

impl DefaultInstanceService for CounterServiceImpl {
    const INSTANCE_ID: u32 = 2;
    const INSTANCE_NAME: &'static str = "test_service";

    fn default_instance(&self) -> InstanceInitParams {
        let version = Version::new(1,1, 1);
        let runtime_id = RuntimeIdentifier::Wasm as u32;
        let artifact_id = ArtifactId::new(runtime_id, "test_service".to_string(), version).unwrap();
        InstanceInitParams::new(Self::INSTANCE_ID, Self::INSTANCE_NAME, artifact_id, vec![])
    }

    fn artifact_id(&self) -> ArtifactId {
        let version = Version::new(1,1, 1);
        let runtime_id = RuntimeIdentifier::Wasm as u32;
        let artifact_id = ArtifactId::new(runtime_id, "test_service".to_string(), version).unwrap();
        artifact_id
    }
}

//// Copyright 2020 The Exonum Team
////
//// Licensed under the Apache License, Version 2.0 (the "License");
//// you may not use this file except in compliance with the License.
//// You may obtain a copy of the License at
////
////   http://www.apache.org/licenses/LICENSE-2.0
////
//// Unless required by applicable law or agreed to in writing, software
//// distributed under the License is distributed on an "AS IS" BASIS,
//// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//// See the License for the specific language governing permissions and
//// limitations under the License.
//
//use exonum::{
//    blockchain::{config::InstanceInitParams, ApiSender, SendError},
//    crypto::{Hash, KeyPair, PublicKey},
//    helpers::{Height, ValidatorId},
//    merkledb::{access::Prefixed, BinaryValue, ObjectHash, Snapshot},
//    runtime::{
//        ArtifactId, BlockchainData, DispatcherAction, ExecutionContext, ExecutionError,
//        InstanceDescriptor, InstanceId, InstanceStatus, Mailbox, MethodId, SnapshotExt,
//    },
//};
//use futures::{
//    executor::block_on,
//    future::{BoxFuture, FutureExt},
//};
//
//use super::{ArtifactProtobufSpec};
//
//use std::fmt::{self, Debug};
//
///// Describes how the service instance should dispatch specific method calls
///// with consideration of the interface where the method belongs.
/////
///// Usually, `ServiceDispatcher` can be derived using the
///// [`ServiceDispatcher`](index.html#examples) macro.
//pub trait ServiceDispatcher: Send {
//    /// Dispatches the interface method call within the specified context.
//    fn call(
//        &self,
//        context: ExecutionContext<'_>,
//        method: MethodId,
//        payload: &[u8],
//    ) -> Result<(), ExecutionError>;
//}
//
///// Describes an Exonum service instance.
/////
///// `Service` determines how a service instance responds to certain requests and events
///// from the runtime.
/////
///// # Implementation Requirements
/////
///// Any changes of the storage state in the methods that can perform such changes (i.e., methods
///// receiving `ExecutionContext`) must be the same for all nodes in the blockchain network.
///// In other words, the service should only use data available in the provided context to perform
///// such changes.
//pub trait Service: ServiceDispatcher + Debug + 'static {
//    /// Initializes a new service instance with the given parameters. This method is called once
//    /// after creating a new service instance.
//    ///
//    /// The default implementation does nothing and returns `Ok(())`.
//    ///
//    /// The parameters passed to the method are not saved by the framework
//    /// automatically, hence the user must do it manually, if needed.
//    fn initialize(
//        &self,
//        _context: ExecutionContext<'_>,
//        _params: Vec<u8>,
//    ) -> Result<(), ExecutionError> {
//        Ok(())
//    }
//
//    /// Resumes a previously stopped service instance with given parameters. This method
//    /// is called once after restarting a service instance.
//    ///
//    /// The default implementation does nothing and returns `Ok(())`.
//    ///
//    /// The parameters passed to the method are not saved by the framework
//    /// automatically, hence the user must do it manually, if needed.
//    ///
//    /// [Migration workflow] guarantees that the data layout is supported by the resumed
//    /// service version.
//    ///
//    /// [Migration workflow]: https://exonum.com/doc/version/latest/architecture/services/#data-migrations
//    fn resume(
//        &self,
//        _context: ExecutionContext<'_>,
//        _params: Vec<u8>,
//    ) -> Result<(), ExecutionError> {
//        Ok(())
//    }
//
//    /// Performs storage operations on behalf of the service before processing any transaction
//    /// in the block.
//    ///
//    /// The default implementation does nothing and returns `Ok(())`.
//    ///
//    /// Services should not rely on a particular ordering of `Service::before_transactions`
//    /// invocations among services.
//    fn before_transactions(&self, _context: ExecutionContext<'_>) -> Result<(), ExecutionError> {
//        Ok(())
//    }
//
//    /// Performs storage operations on behalf of the service after processing all transactions
//    /// in the block.
//    ///
//    /// The default implementation does nothing and returns `Ok(())`.
//    ///
//    /// Note that if service was added in the genesis block, it will be activated immediately and
//    /// thus `after_transactions` will be invoked for such a service after the genesis block creation.
//    /// If you aren't interested in the processing of for the genesis block, you can use
//    /// [`ExecutionContext::in_genesis_block`] method and exit early if `true` is returned.
//    ///
//    /// Invocation of the `height()` method of the core blockchain schema will **panic**
//    /// if invoked within `after_transactions` of the genesis block. If you are going
//    /// to process the genesis block and need to know current height, use the `next_height()` method
//    /// to infer the current blockchain height.
//    ///
//    /// Services should not rely on a particular ordering of `Service::after_transactions`
//    /// invocations among services.
//    ///
//    /// [`ExecutionContext::in_genesis_block`]: struct.ExecutionContext.html#method.in_genesis_block
//    fn after_transactions(&self, _context: ExecutionContext<'_>) -> Result<(), ExecutionError> {
//        Ok(())
//    }
//}
//
///// Describes a service instance factory for the specific Rust artifact.
/////
///// Usually, `ServiceFactory` can be derived using the
///// [`ServiceFactory`](index.html#examples) macro.
//pub trait ServiceFactory: Send + Debug + 'static {
//    /// Returns the unique artifact identifier corresponding to the factory.
//    fn artifact_id(&self) -> ArtifactId;
//    /// Returns the Protobuf specification used by the instances of this service.
//    fn artifact_protobuf_spec(&self) -> ArtifactProtobufSpec;
//    /// Creates a new service instance.
//    fn create_instance(&self) -> Box<dyn Service>;
//}
//
//#[allow(clippy::use_self)] // false positive
//impl<T> From<T> for Box<dyn ServiceFactory>
//    where
//        T: ServiceFactory,
//{
//    fn from(factory: T) -> Self {
//        Box::new(factory) as Self
//    }
//}
//
///// Provides default instance configuration parameters for `ServiceFactory`.
//pub trait DefaultInstance: ServiceFactory {
//    /// Default id for a service.
//    const INSTANCE_ID: InstanceId;
//    /// Default name for a service.
//    const INSTANCE_NAME: &'static str;
//
//    /// Creates default instance configuration parameters for the service.
//    fn default_instance(&self) -> InstanceInitParams {
//        self.artifact_id()
//            .into_default_instance(Self::INSTANCE_ID, Self::INSTANCE_NAME)
//    }
//}
