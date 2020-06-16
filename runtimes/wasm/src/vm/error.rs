use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq,  Deserialize, Serialize)]
pub enum VMError {
    FunctionCallError,
    /// Serialized external error from External trait implementation.
    ExternalError(Vec<u8>),
    /// An error that is caused by an operation on an inconsistent state.
    /// E.g. an integer overflow by using a value from the given context.
    InconsistentStateError,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum VMLogicError {
    HostError,
    /// Serialized external error from External trait implementation.
    ExternalError(Vec<u8>),
    /// An error that is caused by an operation on an inconsistent state.
    InconsistentStateError,
}
