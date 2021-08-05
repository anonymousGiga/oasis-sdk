//! Processing of execution results.
use std::convert::TryInto;

use oasis_contract_sdk_types::{
    message::{Message, NotifyReply, Reply},
    ExecutionOk,
};
use oasis_runtime_sdk::{
    context::{BatchContext, Context, TxContext},
    dispatcher,
    modules::core::{self, API as _},
    types::{token, transaction},
};

use crate::{wasm, Error, Parameters};

/// Context key used for tracking the execution call depth to make sure that the maximum depth is
/// not exceeded as that could result in a stack overflow.
const CONTEXT_KEY_DEPTH: &str = "contracts.CallDepth";

pub(crate) fn process_execution_result<C: TxContext>(
    ctx: &mut C,
    params: &Parameters,
    contract: &wasm::Contract<'_>,
    result: ExecutionOk,
) -> Result<Vec<u8>, Error> {
    // Ensure the call depth is not too large. Note that gas limits should prevent this growing
    // overly large, but as a defense in depth we also enforce limits.
    let current_depth: u16 = *ctx.value(CONTEXT_KEY_DEPTH).or_default();
    if !result.messages.is_empty() && current_depth >= params.max_subcall_depth {
        return Err(Error::CallDepthExceeded(
            current_depth + 1,
            params.max_subcall_depth,
        ));
    }

    // By default the resulting data is what the call returned. Message reply processing may
    // overwrite this data when it is non-empty.
    let mut result_data = result.data;

    // Process events.
    // TODO

    // Charge gas for each emitted message.
    core::Module::use_tx_gas(
        ctx,
        params
            .gas_costs
            .subcall_dispatch
            .saturating_mul(result.messages.len() as u64),
    )?;

    // Make sure the number of subcalls is within limits.
    let message_count = result
        .messages
        .len()
        .try_into()
        .map_err(|_| Error::TooManySubcalls(u16::MAX, params.max_subcall_count))?;
    if message_count > params.max_subcall_count {
        return Err(Error::TooManySubcalls(
            message_count,
            params.max_subcall_count,
        ));
    }

    // Process emitted messages recursively.
    for msg in result.messages {
        match msg {
            Message::Call {
                id,
                reply,
                method,
                body,
                max_gas,
            } => {
                // Calculate how much gas the child message can use.
                let remaining_gas = core::Module::remaining_gas(ctx);
                let max_gas = max_gas.unwrap_or(remaining_gas);
                let max_gas = if max_gas > remaining_gas {
                    remaining_gas
                } else {
                    max_gas
                };

                // Execute a transaction in a child context.
                let (result, gas, tags, messages) = ctx.with_child(ctx.mode(), |mut ctx| {
                    // Generate fake transaction.
                    let tx = transaction::Transaction {
                        version: transaction::LATEST_TRANSACTION_VERSION,
                        call: transaction::Call { method, body },
                        auth_info: transaction::AuthInfo {
                            signer_info: vec![transaction::SignerInfo {
                                // The call is being performed on the contract's behalf.
                                address_spec: transaction::AddressSpec::Internal(
                                    contract.instance_info.address(),
                                ),
                                nonce: 0,
                            }],
                            fee: transaction::Fee {
                                amount: token::BaseUnits::new(0, token::Denomination::NATIVE),
                                // Limit gas usage inside the child context to the allocated maximum.
                                gas: max_gas,
                            },
                        },
                    };

                    let result = ctx.with_tx(tx, |mut ctx, call| {
                        // Propagate call depth.
                        ctx.value(CONTEXT_KEY_DEPTH).set(current_depth + 1);

                        // Dispatch the call.
                        let result =
                            dispatcher::Dispatcher::<C::Runtime>::dispatch_tx_call(&mut ctx, call);
                        // Retrieve remaining gas.
                        let gas = core::Module::remaining_gas(&mut ctx);

                        // Commit store and return emitted tags and messages on successful dispatch,
                        // otherwise revert state and ignore any emitted events/messages.
                        if result.is_success() {
                            let (tags, messages) = ctx.commit();
                            (result, gas, tags, messages)
                        } else {
                            // Ignore tags/messages on failure.
                            (result, gas, vec![], vec![])
                        }
                    });

                    // Commit storage. Note that if child context didn't commit, this is
                    // basically a no-op.
                    ctx.commit();

                    result
                });

                // Use any gas that was used inside the child context. This should never fail as we
                // preconfigured the amount of available gas.
                core::Module::use_tx_gas(ctx, max_gas.saturating_sub(gas))?;

                // Forward any emitted tags.
                for tag in tags {
                    ctx.emit_tag(tag);
                }

                // Forward any emitted runtime messages.
                for (msg, hook) in messages {
                    // This should never fail as child context has the right limits configured.
                    ctx.emit_message(msg, hook)?;
                }

                // Process replies based on filtering criteria.
                match (reply, result.is_success()) {
                    (NotifyReply::OnError, false)
                    | (NotifyReply::OnSuccess, true)
                    | (NotifyReply::Always, _) => {
                        // Construct and process reply.
                        let reply = Reply::Call {
                            id,
                            result: result.into(),
                        };
                        let reply_result = wasm::handle_reply(ctx, &params, &contract, reply)?;
                        let reply_result =
                            process_execution_result(ctx, &params, &contract, reply_result)?;

                        // If there is a non-empty reply, it overwrites the returned data.
                        if !reply_result.is_empty() {
                            result_data = reply_result;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(result_data)
}
