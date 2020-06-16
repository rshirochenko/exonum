use std::{cell::Cell};

use exonum::blockchain::config::InstanceInitParams;
use exonum::runtime::ExecutionError;
use exonum::{
    runtime::{versioning::Version, ArtifactId, InstanceId, RuntimeIdentifier},
};
use wasmer_runtime::{imports, Instance as WasmInstance, func, ImportObject};

use std::collections::HashMap;
use std::fs;
use std::{cell::RefCell, sync::Arc};

use crate::vm::{
    error::{VMError, VMLogicError},
    logic::{ReturnData, VMLogic},
    imports,
    memory::WasmerMemory,
};

pub const MODULES_PATH: &'static str = "/Users/roman/Documents/exonum/exonum/target/wasm32-unknown-unknown/debug";

/// Service instance with a counter.
#[derive(Debug, Default, Clone)]
pub struct CounterService {
    pub counter: Cell<u64>,
    pub name: String,
}

#[derive(Debug, Default, Clone)]
pub struct WasmService {
    pub name: String,
    pub wasm_bytes: Vec<u8>,
}

impl WasmService {
    pub fn new(name: &str) -> Result<Self, std::io::Error> {
        let module_path = format!("{}/wasm_counter_service.wasm", MODULES_PATH);
        let wasm_bytes = fs::read(module_path)?;
        Ok(Self {
            name: name.to_string(),
            wasm_bytes: wasm_bytes,
        })
    }

    pub fn instantiate(&self) -> WasmInstance {
        let import_object = self.form_import_objects();
        let instance = wasmer_runtime::instantiate(&self.wasm_bytes, &import_object).unwrap();
        instance
    }

    fn form_import_objects(&self) -> ImportObject {
        fn add_to_counter(counter: i32, add_value: i32) -> i32 {
            counter + add_value
        }

        imports! {
            "host" => {
                "add_to_counter" => func!(add_to_counter),
            },
        }
    }

    pub fn run(&self, method_name: &[u8]) -> (Option<ReturnData>, Option<VMError>) {
        let mut memory = match WasmerMemory::new(
            10 as u32,
            10 as u32,
        ) {
            Ok(x) => x,
            Err(_err) => panic!("Cannot create memory for a contract call"),
        };
        let memory_copy = memory.clone();

        let mut logic = VMLogic::new(&mut memory);

        //let import_object = imports::build(memory_copy, &mut logic);

        let method_name = match std::str::from_utf8(method_name) {
            Ok(x) => x,
            Err(_) => panic!("cannot parse method name"),
        };

        let import_object = self.form_import_objects();

        match wasmer_runtime::instantiate(&self.wasm_bytes, &import_object) {
            Ok(instance) => match instance.call(&method_name, &[]) {
                Ok(_) => (Some(logic.outcome()), None),
                Err(err) => (None, Some(VMError::FunctionCallError)),
            },
            Err(err) => panic!("wasm execution error"),
        }
    }
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
