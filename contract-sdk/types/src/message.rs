//! Messages that can be emitted by contracts.

/// Messages can be emitted by contracts and are processed after the contract execution completes.
#[derive(Clone, Debug, cbor::Encode, cbor::Decode)]
pub enum Message {
    /// Calls an arbitrary runtime method handler in a child context with an optional gas limit.
    ///
    /// The call is executed in the context of the smart contract as the caller within the same
    /// transaction.
    ///
    /// This can be used to call other smart contracts.
    #[cbor(rename = "call")]
    Call {
        #[cbor(optional, default)]
        id: u64,
        reply: NotifyReply,
        method: String,
        body: cbor::Value,
        #[cbor(optional)]
        max_gas: Option<u64>,
    },
}

/// Specifies when the caller (smart contract) wants to be notified of a reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, cbor::Encode, cbor::Decode)]
#[repr(u8)]
pub enum NotifyReply {
    Never = 0,
    OnError = 1,
    OnSuccess = 2,
    Always = 3,
}

/// Replies to delivered messages.
#[derive(Clone, Debug, cbor::Encode, cbor::Decode)]
pub enum Reply {
    /// Reply from a call message.
    #[cbor(rename = "call")]
    Call {
        #[cbor(optional, default)]
        id: u64,
        result: CallResult,
    },
}

/// Call result.
#[derive(Clone, Debug, cbor::Encode, cbor::Decode)]
pub enum CallResult {
    #[cbor(rename = "ok")]
    Ok(cbor::Value),

    #[cbor(rename = "fail")]
    Failed { module: String, code: u32 },
}

impl CallResult {
    /// Check whether the call result indicates a successful operation or not.
    pub fn is_success(&self) -> bool {
        match self {
            CallResult::Ok(_) => true,
            CallResult::Failed { .. } => false,
        }
    }
}

#[cfg(feature = "oasis-runtime-sdk")]
impl From<oasis_runtime_sdk::types::transaction::CallResult> for CallResult {
    fn from(r: oasis_runtime_sdk::types::transaction::CallResult) -> Self {
        match r {
            oasis_runtime_sdk::types::transaction::CallResult::Ok(value) => Self::Ok(value),
            oasis_runtime_sdk::types::transaction::CallResult::Failed { module, code, .. } => {
                Self::Failed { module, code }
            }
        }
    }
}
