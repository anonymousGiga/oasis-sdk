//! Smart contracts module.
#![deny(rust_2018_idioms)]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate alloc;

use std::convert::TryInto;

use thiserror::Error;

use oasis_runtime_sdk::{
    self as sdk,
    context::{Context, TxContext},
    core::common::crypto::hash::Hash,
    error::{self, Error as _},
    module,
    module::Module as _,
    modules,
    modules::core::{Module as Core, API as _},
    storage::{self, Store as _},
    types::transaction::CallResult,
};

mod abi;
mod subcalls;
#[cfg(test)]
mod test;
pub mod types;
mod wasm;

/// Unique module name.
const MODULE_NAME: &str = "contracts";

/// Errors emitted by the contracts module.
#[derive(Error, Debug, sdk::Error)]
pub enum Error {
    #[error("invalid argument")]
    #[sdk_error(code = 1)]
    InvalidArgument,

    #[error("code too large (size: {0} max: {1})")]
    #[sdk_error(code = 2)]
    CodeTooLarge(u32, u32),

    #[error("code is malformed")]
    #[sdk_error(code = 3)]
    CodeMalformed,

    #[error("specified ABI is not supported")]
    #[sdk_error(code = 4)]
    UnsupportedABI,

    #[error("code is missing required ABI export: {0}")]
    #[sdk_error(code = 5)]
    CodeMissingRequiredExport(String),

    #[error("code declares reserved ABI export: {0}")]
    #[sdk_error(code = 6)]
    CodeDeclaresReservedExport(String),

    #[error("code declares start function")]
    #[sdk_error(code = 7)]
    CodeDeclaresStartFunction,

    #[error("code declares too many memories")]
    #[sdk_error(code = 8)]
    CodeDeclaresTooManyMemories,

    #[error("code not found")]
    #[sdk_error(code = 9)]
    CodeNotFound,

    #[error("instance not found")]
    #[sdk_error(code = 10)]
    InstanceNotFound,

    #[error("module loading failed")]
    #[sdk_error(code = 11)]
    ModuleLoadingFailed,

    #[error("execution failed: {0}")]
    #[sdk_error(code = 12)]
    ExecutionFailed(#[source] anyhow::Error),

    #[error("forbidden by policy")]
    #[sdk_error(code = 13)]
    Forbidden,

    #[error("function not supported")]
    #[sdk_error(code = 14)]
    Unsupported,

    #[error("insufficient balance in caller account")]
    #[sdk_error(code = 15)]
    InsufficientCallerBalance,

    #[error("call depth exceeded (depth: {0} max: {1})")]
    #[sdk_error(code = 16)]
    CallDepthExceeded(u16, u16),

    #[error("result size exceeded (size: {0} max: {1})")]
    #[sdk_error(code = 17)]
    ResultTooLarge(u32, u32),

    #[error("too many subcalls (count: {0} max: {1})")]
    #[sdk_error(code = 18)]
    TooManySubcalls(u16, u16),

    #[error("core: {0}")]
    #[sdk_error(transparent)]
    Core(#[from] modules::core::Error),

    #[error("contract error: {0}")]
    #[sdk_error(transparent)]
    Contract(#[from] wasm::ContractError),
}

/// Events emitted by the contracts module.
#[derive(Debug, cbor::Encode, sdk::Event)]
#[cbor(untagged)]
pub enum Event {}

/// Gas costs.
#[derive(Clone, Debug, cbor::Encode, cbor::Decode)]
pub struct GasCosts {
    pub tx_upload: u64,
    pub tx_upload_per_byte: u64,
    pub tx_instantiate: u64,
    pub tx_call: u64,
    pub tx_upgrade: u64,

    // Subcalls.
    pub subcall_dispatch: u64,

    // Storage operations.
    pub wasm_storage_get_base: u64,
    pub wasm_storage_insert_base: u64,
    pub wasm_storage_remove_base: u64,
    // TODO: Costs of storage operations.
    // TODO: Cost of emitted messages.
    // TODO: Cost of queries.
}

impl Default for GasCosts {
    fn default() -> Self {
        // TODO: Decide what reasonable defaults should be.
        GasCosts {
            tx_upload: 0,
            tx_upload_per_byte: 0,
            tx_instantiate: 0,
            tx_call: 0,
            tx_upgrade: 0,

            subcall_dispatch: 100,

            wasm_storage_get_base: 10,
            wasm_storage_insert_base: 10,
            wasm_storage_remove_base: 10,
        }
    }
}

/// Parameters for the contracts module.
#[derive(Clone, Debug, cbor::Encode, cbor::Decode)]
pub struct Parameters {
    pub max_code_size: u32,
    pub max_stack_size: u32,
    pub max_memory_pages: u32,
    pub max_subcall_depth: u16,
    pub max_subcall_count: u16,
    pub max_result_size_bytes: u32,

    pub gas_costs: GasCosts,
}

impl Default for Parameters {
    fn default() -> Self {
        // TODO: Decide what reasonable defaults should be.
        Parameters {
            max_code_size: 256 * 1024, // 256 KiB
            max_stack_size: 60 * 1024, // 60 KiB
            max_memory_pages: 20,      // 1280 KiB
            max_subcall_depth: 8,
            max_subcall_count: 16,
            max_result_size_bytes: 1024, // 1 KiB

            gas_costs: Default::default(),
        }
    }
}

impl module::Parameters for Parameters {
    type Error = std::convert::Infallible;
}

/// Genesis state for the contracts module.
#[derive(Clone, Debug, Default, cbor::Encode, cbor::Decode)]
pub struct Genesis {
    pub parameters: Parameters,
}

/// Interface that can be called from other modules.
pub trait API {
    // TODO: What makes sense?
}

/// State schema constants.
pub mod state {
    /// Next code identifier (u64).
    pub const NEXT_CODE_IDENTIFIER: &[u8] = &[0x01];
    /// Next instance identifier (u64).
    pub const NEXT_INSTANCE_IDENTIFIER: &[u8] = &[0x02];
    /// Information about uploaded code.
    pub const CODE_INFO: &[u8] = &[0x03];
    /// Information about the deployed contract instance.
    pub const INSTANCE_INFO: &[u8] = &[0x04];
    /// Per-instance key/value store.
    pub const INSTANCE_STATE: &[u8] = &[0x05];

    /// Uploaded code.
    pub const CODE: &[u8] = &[0xFF];
}

pub struct Module<Accounts: modules::accounts::API> {
    _accounts: std::marker::PhantomData<Accounts>,
}

impl<Accounts: modules::accounts::API> Module<Accounts> {
    /// Loads code information for the specified code identifier.
    fn load_code_info<C: Context>(
        ctx: &mut C,
        code_id: types::CodeId,
    ) -> Result<types::Code, Error> {
        let mut store = storage::PrefixStore::new(ctx.runtime_state(), &MODULE_NAME);
        let code_info_store =
            storage::TypedStore::new(storage::PrefixStore::new(&mut store, &state::CODE_INFO));
        let code_info: types::Code = code_info_store
            .get(code_id.to_storage_key())
            .ok_or(Error::CodeNotFound)?;

        Ok(code_info)
    }

    /// Stores specified code information.
    fn store_code_info<C: Context>(ctx: &mut C, code_info: types::Code) -> Result<(), Error> {
        let mut store = storage::PrefixStore::new(ctx.runtime_state(), &MODULE_NAME);
        let mut code_info_store =
            storage::TypedStore::new(storage::PrefixStore::new(&mut store, &state::CODE_INFO));
        code_info_store.insert(code_info.id.to_storage_key(), code_info);

        Ok(())
    }

    /// Loads code with the specified code identifier.
    fn load_code<C: Context>(ctx: &mut C, code_id: types::CodeId) -> Result<Vec<u8>, Error> {
        // TODO: Spport local untrusted cache to avoid storage queries.
        let mut store = storage::PrefixStore::new(ctx.runtime_state(), &MODULE_NAME);
        let code_store = storage::PrefixStore::new(&mut store, &state::CODE);
        let code = code_store
            .get(&code_id.to_storage_key())
            .ok_or(Error::CodeNotFound)?;

        Ok(code)
    }

    /// Stores code with the specified code identifier.
    fn store_code<C: Context>(
        ctx: &mut C,
        code_id: types::CodeId,
        code: &[u8],
    ) -> Result<(), Error> {
        let mut store = storage::PrefixStore::new(ctx.runtime_state(), &MODULE_NAME);
        let mut code_store = storage::PrefixStore::new(&mut store, &state::CODE);
        code_store.insert(&code_id.to_storage_key(), &code);

        Ok(())
    }

    /// Loads specified instance information.
    fn load_instance_info<C: Context>(
        ctx: &mut C,
        instance_id: types::InstanceId,
    ) -> Result<types::Instance, Error> {
        let mut store = storage::PrefixStore::new(ctx.runtime_state(), &MODULE_NAME);
        let instance_info_store =
            storage::TypedStore::new(storage::PrefixStore::new(&mut store, &state::INSTANCE_INFO));
        let instance_info = instance_info_store
            .get(instance_id.to_storage_key())
            .ok_or(Error::InstanceNotFound)?;

        Ok(instance_info)
    }

    /// Stores specified instance information.
    fn store_instance_info<C: Context>(
        ctx: &mut C,
        instance_info: types::Instance,
    ) -> Result<(), Error> {
        let mut store = storage::PrefixStore::new(ctx.runtime_state(), &MODULE_NAME);
        let mut instance_info_store =
            storage::TypedStore::new(storage::PrefixStore::new(&mut store, &state::INSTANCE_INFO));
        instance_info_store.insert(instance_info.id.to_storage_key(), instance_info);

        Ok(())
    }
}

impl<Accounts: modules::accounts::API> Module<Accounts> {
    fn tx_upload<C: TxContext>(
        ctx: &mut C,
        body: types::Upload,
    ) -> Result<types::UploadResult, Error> {
        let params = Self::params(ctx.runtime_state());

        // Validate code size.
        let code_size: u32 = body
            .code
            .len()
            .try_into()
            .map_err(|_| Error::CodeTooLarge(u32::MAX, params.max_code_size))?;
        if code_size > params.max_code_size {
            return Err(Error::CodeTooLarge(code_size, params.max_code_size));
        }

        Core::use_tx_gas(ctx, params.gas_costs.tx_upload)?;
        Core::use_tx_gas(
            ctx,
            params
                .gas_costs
                .tx_upload_per_byte
                .saturating_mul(body.code.len() as u64),
        )?;

        if ctx.is_check_only() && !ctx.are_expensive_queries_allowed() {
            // Only fast checks are allowed.
            return Ok(types::UploadResult::default());
        }

        // Validate and transform the code.
        let code = wasm::validate_and_transform(ctx, &params, &body.code, body.abi)?;
        let hash = Hash::digest_bytes(&code);

        // Validate code size again and account for any instrumentation. This is here to avoid any
        // incentives in generating code that gets maximally inflated after instrumentation.
        let inst_code_size: u32 = code
            .len()
            .try_into()
            .map_err(|_| Error::CodeTooLarge(u32::MAX, params.max_code_size))?;
        if inst_code_size > params.max_code_size {
            return Err(Error::CodeTooLarge(inst_code_size, params.max_code_size));
        }
        Core::use_tx_gas(
            ctx,
            params
                .gas_costs
                .tx_upload_per_byte
                .saturating_mul(inst_code_size.saturating_sub(code_size) as u64),
        )?;

        if ctx.is_check_only() {
            return Ok(types::UploadResult::default());
        }

        // Assign next identifier.
        let mut store = storage::PrefixStore::new(ctx.runtime_state(), &MODULE_NAME);
        let mut tstore = storage::TypedStore::new(&mut store);
        let id: types::CodeId = tstore.get(state::NEXT_CODE_IDENTIFIER).unwrap_or_default();
        tstore.insert(state::NEXT_CODE_IDENTIFIER, id.increment());

        // Store information about uploaded code.
        Self::store_code_info(
            ctx,
            types::Code {
                id,
                hash,
                abi: body.abi,
                instantiate_policy: body.instantiate_policy,
            },
        )?;
        Self::store_code(ctx, id, &code)?;

        Ok(types::UploadResult { id })
    }

    fn tx_instantiate<C: TxContext>(
        ctx: &mut C,
        body: types::Instantiate,
    ) -> Result<types::InstantiateResult, Error> {
        let params = Self::params(ctx.runtime_state());
        let creator = ctx.tx_caller_address();

        Core::use_tx_gas(ctx, params.gas_costs.tx_instantiate)?;

        if ctx.is_check_only() && !ctx.are_expensive_queries_allowed() {
            // Only fast checks are allowed.
            return Ok(types::InstantiateResult::default());
        }

        // Load code information, enforce instantiation policy and load the code.
        let code_info = Self::load_code_info(ctx, body.code_id)?;
        code_info.instantiate_policy.enforce(ctx)?;
        let code = Self::load_code(ctx, body.code_id)?;

        // Assign next identifier.
        let mut store = storage::PrefixStore::new(ctx.runtime_state(), &MODULE_NAME);
        let mut tstore = storage::TypedStore::new(&mut store);
        let id: types::InstanceId = tstore
            .get(state::NEXT_INSTANCE_IDENTIFIER)
            .unwrap_or_default();
        tstore.insert(state::NEXT_INSTANCE_IDENTIFIER, id.increment());

        // Store instance information.
        let instance_info = types::Instance {
            id,
            code_id: body.code_id,
            creator,
            upgrades_policy: body.upgrades_policy,
        };
        Self::store_instance_info(ctx, instance_info.clone())?;

        // Transfer any attached tokens.
        for tokens in &body.tokens {
            Accounts::transfer(ctx, creator, instance_info.address(), tokens)
                .map_err(|_| Error::InsufficientCallerBalance)?
        }
        // Run instantiation function.
        let contract = wasm::Contract {
            code_info: &code_info,
            code: &code,
            instance_info: &instance_info,
        };
        let result = wasm::instantiate(ctx, &params, &contract, &body)?;
        subcalls::process_execution_result(ctx, &params, &contract, result)?;

        Ok(types::InstantiateResult { id })
    }

    fn tx_call<C: TxContext>(ctx: &mut C, body: types::Call) -> Result<types::CallResult, Error> {
        let params = Self::params(ctx.runtime_state());
        let caller = ctx.tx_caller_address();

        Core::use_tx_gas(ctx, params.gas_costs.tx_call)?;

        if ctx.is_check_only() && !ctx.are_expensive_queries_allowed() {
            // Only fast checks are allowed.
            return Ok(types::CallResult::default());
        }

        // Load instance information and code.
        let instance_info = Self::load_instance_info(ctx, body.id)?;
        let code_info = Self::load_code_info(ctx, instance_info.code_id)?;
        let code = Self::load_code(ctx, instance_info.code_id)?;

        // Transfer any attached tokens.
        for tokens in &body.tokens {
            Accounts::transfer(ctx, caller, instance_info.address(), tokens)
                .map_err(|_| Error::InsufficientCallerBalance)?
        }
        // Run call function.
        let contract = wasm::Contract {
            code_info: &code_info,
            code: &code,
            instance_info: &instance_info,
        };
        let result = wasm::call(ctx, &params, &contract, &body)?;
        let data = subcalls::process_execution_result(ctx, &params, &contract, result)?;

        Ok(types::CallResult(data))
    }

    fn tx_upgrade<C: TxContext>(ctx: &mut C, _body: types::Upgrade) -> Result<(), Error> {
        let params = Self::params(ctx.runtime_state());

        Core::use_tx_gas(ctx, params.gas_costs.tx_upgrade)?;

        Err(Error::Unsupported)
    }

    fn query_code<C: Context>(ctx: &mut C, args: types::CodeQuery) -> Result<types::Code, Error> {
        Self::load_code_info(ctx, args.id)
    }

    fn query_instance<C: Context>(
        ctx: &mut C,
        args: types::InstanceQuery,
    ) -> Result<types::Instance, Error> {
        Self::load_instance_info(ctx, args.id)
    }

    fn query_instance_storage<C: Context>(
        _ctx: &mut C,
        _args: types::InstanceStorageQuery,
    ) -> Result<types::InstanceStorageQueryResult, Error> {
        Err(Error::Unsupported)
    }

    fn query_public_key<C: Context>(
        _ctx: &mut C,
        _args: types::PublicKeyQuery,
    ) -> Result<types::PublicKeyQueryResult, Error> {
        Err(Error::Unsupported)
    }

    fn query_custom<C: Context>(
        _ctx: &mut C,
        _args: types::CustomQuery,
    ) -> Result<types::CustomQueryResult, Error> {
        Err(Error::Unsupported)
    }
}

impl<Accounts: modules::accounts::API> module::Module for Module<Accounts> {
    const NAME: &'static str = MODULE_NAME;
    type Error = Error;
    type Event = Event;
    type Parameters = Parameters;
}

impl<Accounts: modules::accounts::API> module::MethodHandler for Module<Accounts> {
    fn dispatch_call<C: TxContext>(
        ctx: &mut C,
        method: &str,
        body: cbor::Value,
    ) -> module::DispatchResult<cbor::Value, CallResult> {
        match method {
            "contracts.Upload" => {
                let result = || -> Result<cbor::Value, Error> {
                    let args = cbor::from_value(body).map_err(|_| Error::InvalidArgument)?;
                    Ok(cbor::to_value(Self::tx_upload(ctx, args)?))
                }();
                match result {
                    Ok(value) => module::DispatchResult::Handled(CallResult::Ok(value)),
                    Err(err) => module::DispatchResult::Handled(err.to_call_result()),
                }
            }
            "contracts.Instantiate" => {
                let result = || -> Result<cbor::Value, Error> {
                    let args = cbor::from_value(body).map_err(|_| Error::InvalidArgument)?;
                    Ok(cbor::to_value(Self::tx_instantiate(ctx, args)?))
                }();
                match result {
                    Ok(value) => module::DispatchResult::Handled(CallResult::Ok(value)),
                    Err(err) => module::DispatchResult::Handled(err.to_call_result()),
                }
            }
            "contracts.Call" => {
                let result = || -> Result<cbor::Value, Error> {
                    let args = cbor::from_value(body).map_err(|_| Error::InvalidArgument)?;
                    Ok(cbor::to_value(Self::tx_call(ctx, args)?))
                }();
                match result {
                    Ok(value) => module::DispatchResult::Handled(CallResult::Ok(value)),
                    Err(err) => module::DispatchResult::Handled(err.to_call_result()),
                }
            }
            "contracts.Upgrade" => {
                let result = || -> Result<cbor::Value, Error> {
                    let args = cbor::from_value(body).map_err(|_| Error::InvalidArgument)?;
                    Ok(cbor::to_value(Self::tx_upgrade(ctx, args)?))
                }();
                match result {
                    Ok(value) => module::DispatchResult::Handled(CallResult::Ok(value)),
                    Err(err) => module::DispatchResult::Handled(err.to_call_result()),
                }
            }
            _ => module::DispatchResult::Unhandled(body),
        }
    }

    fn dispatch_query<C: Context>(
        ctx: &mut C,
        method: &str,
        args: cbor::Value,
    ) -> module::DispatchResult<cbor::Value, Result<cbor::Value, error::RuntimeError>> {
        match method {
            "contracts.Code" => module::DispatchResult::Handled((|| {
                let args = cbor::from_value(args).map_err(|_| Error::InvalidArgument)?;
                Ok(cbor::to_value(Self::query_code(ctx, args)?))
            })()),
            "contracts.Instance" => module::DispatchResult::Handled((|| {
                let args = cbor::from_value(args).map_err(|_| Error::InvalidArgument)?;
                Ok(cbor::to_value(Self::query_instance(ctx, args)?))
            })()),
            "contracts.InstanceStorage" => module::DispatchResult::Handled((|| {
                let args = cbor::from_value(args).map_err(|_| Error::InvalidArgument)?;
                Ok(cbor::to_value(Self::query_instance_storage(ctx, args)?))
            })()),
            "contracts.PublicKey" => module::DispatchResult::Handled((|| {
                let args = cbor::from_value(args).map_err(|_| Error::InvalidArgument)?;
                Ok(cbor::to_value(Self::query_public_key(ctx, args)?))
            })()),
            "contracts.Custom" => module::DispatchResult::Handled((|| {
                let args = cbor::from_value(args).map_err(|_| Error::InvalidArgument)?;
                Ok(cbor::to_value(Self::query_custom(ctx, args)?))
            })()),
            _ => module::DispatchResult::Unhandled(args),
        }
    }
}

impl<Accounts: modules::accounts::API> Module<Accounts> {
    /// Initialize state from genesis.
    fn init<C: Context>(ctx: &mut C, genesis: Genesis) {
        // Set genesis parameters.
        Self::set_params(ctx.runtime_state(), genesis.parameters);
    }

    /// Migrate state from a previous version.
    fn migrate<C: Context>(_ctx: &mut C, _from: u32) -> bool {
        // No migrations currently supported.
        false
    }
}

impl<Accounts: modules::accounts::API> module::MigrationHandler for Module<Accounts> {
    type Genesis = Genesis;

    fn init_or_migrate<C: Context>(
        ctx: &mut C,
        meta: &mut modules::core::types::Metadata,
        genesis: Self::Genesis,
    ) -> bool {
        let version = meta.versions.get(Self::NAME).copied().unwrap_or_default();
        if version == 0 {
            // Initialize state from genesis.
            Self::init(ctx, genesis);
            meta.versions.insert(Self::NAME.to_owned(), Self::VERSION);
            return true;
        }

        // Perform migration.
        Self::migrate(ctx, version)
    }
}

impl<Accounts: modules::accounts::API> module::AuthHandler for Module<Accounts> {}
impl<Accounts: modules::accounts::API> module::BlockHandler for Module<Accounts> {}
impl<Accounts: modules::accounts::API> module::InvariantHandler for Module<Accounts> {}
