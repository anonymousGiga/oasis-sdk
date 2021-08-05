//! Tests for Oasis ABIs.
use oasis_runtime_sdk::{
    context::BatchContext, core::common::crypto::hash::Hash, error::Error as _, modules::core,
    testing::mock, types::address::Address,
};

use crate::{types, wasm, Error, Parameters};

#[test]
fn test_validate_and_transform() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();
    let params = Parameters::default();

    ctx.with_tx(mock::transaction(), |mut ctx, _| {
        // Non-WASM code.
        let code = Vec::new();
        let result = wasm::validate_and_transform(&mut ctx, &params, &code, types::ABI::OasisV1);
        assert!(
            matches!(result, Err(Error::CodeMalformed)),
            "malformed code shoud fail validation"
        );

        // WASM code but without the required exports.
        let code = [
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60, 0x01, 0x7f,
            0x01, 0x7f, 0x03, 0x02, 0x01, 0x00, 0x07, 0x07, 0x01, 0x03, 0x66, 0x69, 0x62, 0x00,
            0x00, 0x0a, 0x1f, 0x01, 0x1d, 0x00, 0x20, 0x00, 0x41, 0x02, 0x49, 0x04, 0x40, 0x20,
            0x00, 0x0f, 0x0b, 0x20, 0x00, 0x41, 0x02, 0x6b, 0x10, 0x00, 0x20, 0x00, 0x41, 0x01,
            0x6b, 0x10, 0x00, 0x6a, 0x0f, 0x0b,
        ];
        let result = wasm::validate_and_transform(&mut ctx, &params, &code, types::ABI::OasisV1);
        assert!(
            matches!(result, Err(Error::CodeMissingRequiredExport(_))),
            "valid WASM, but non-ABI conformant code should fail validation"
        );

        // WASM code with required exports.
        let code = wat::parse_str(
            r#"
            (module
                (type (;0;) (func))
                (func (;0;) (type 0))

                (export "allocate" (func 0))
                (export "deallocate" (func 0))
                (export "instantiate" (func 0))
                (export "call" (func 0))
            )
        "#,
        )
        .unwrap();
        let result = wasm::validate_and_transform(&mut ctx, &params, &code, types::ABI::OasisV1);
        assert!(
            result.is_ok(),
            "valid WASM with required exports should be ok"
        );

        // WASM code with reserved exports.
        let code = wat::parse_str(
            r#"
            (module
                (type (;0;) (func))
                (func (;0;) (type 0))

                (export "allocate" (func 0))
                (export "deallocate" (func 0))
                (export "instantiate" (func 0))
                (export "call" (func 0))
                (export "gas_limit" (func 0))
            )
        "#,
        )
        .unwrap();
        let result = wasm::validate_and_transform(&mut ctx, &params, &code, types::ABI::OasisV1);
        assert!(
            matches!(result, Err(Error::CodeDeclaresReservedExport(_))),
            "valid WASM, but non-ABI conformant code should fail validation"
        );

        // WASM code with start function defined.
        let code = wat::parse_str(
            r#"
            (module
                (type (;0;) (func))
                (func (;0;) (type 0))

                (start 0)
                (export "allocate" (func 0))
                (export "deallocate" (func 0))
                (export "instantiate" (func 0))
                (export "call" (func 0))
            )
        "#,
        )
        .unwrap();
        let result = wasm::validate_and_transform(&mut ctx, &params, &code, types::ABI::OasisV1);
        assert!(
            matches!(result, Err(Error::CodeDeclaresStartFunction)),
            "WASM with start function defined should fail validation"
        );

        // WASM code with multiple memories defined.
        let code = wat::parse_str(
            r#"
            (module
                (type (;0;) (func))
                (func (;0;) (type 0))

                (memory $m1 17)
                (memory $m2 17)
                (export "allocate" (func 0))
                (export "deallocate" (func 0))
                (export "instantiate" (func 0))
                (export "call" (func 0))
            )
        "#,
        )
        .unwrap();
        let result = wasm::validate_and_transform(&mut ctx, &params, &code, types::ABI::OasisV1);
        assert!(
            matches!(result, Err(Error::CodeDeclaresTooManyMemories)),
            "WASM with multiple memories defined should fail validation"
        );
    });
}

fn run_contract_with_defaults(
    code: &[u8],
    gas_limit: u64,
    instantiate_data: cbor::Value,
    call_data: cbor::Value,
) -> Result<cbor::Value, Error> {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();
    let params = Parameters::default();

    core::Module::init(
        &mut ctx,
        core::Genesis {
            parameters: core::Parameters {
                max_batch_gas: gas_limit,
                ..Default::default()
            },
        },
    );

    let mut tx = mock::transaction();
    tx.auth_info.fee.gas = gas_limit;

    ctx.with_tx(tx, |mut ctx, _| -> Result<cbor::Value, Error> {
        let code =
            wasm::validate_and_transform(&mut ctx, &params, code, types::ABI::OasisV1).unwrap();

        let code_info = types::Code {
            id: 1.into(),
            hash: Hash::empty_hash(),
            abi: types::ABI::OasisV1,
            instantiate_policy: types::Policy::Everyone,
        };
        let call = types::Instantiate {
            code_id: code_info.id,
            upgrades_policy: types::Policy::Everyone,
            data: cbor::to_vec(instantiate_data),
            tokens: vec![],
        };
        let instance_info = types::Instance {
            id: 1.into(),
            code_id: 1.into(),
            creator: Address::default(),
            upgrades_policy: call.upgrades_policy,
        };

        // Instantiate the contract.
        let contract = wasm::Contract {
            code_info: &code_info,
            code: &code,
            instance_info: &instance_info,
        };
        wasm::instantiate(&mut ctx, &params, &contract, &call)?;

        // Call the contract.
        let call = types::Call {
            id: 1.into(),
            data: cbor::to_vec(call_data),
            tokens: vec![],
        };
        let result = wasm::call(&mut ctx, &params, &contract, &call)?;
        let result: cbor::Value =
            cbor::from_slice(&result.data).map_err(|err| Error::ExecutionFailed(err.into()))?;

        Ok(result)
    })
}

#[test]
fn test_hello_contract() {
    let code = include_bytes!("../../../../../../tests/contracts/hello/hello.wasm");
    let result = run_contract_with_defaults(
        &code[..],
        1_000_000,
        cbor::cbor_text!("instantiate"),
        cbor::cbor_map! { "say_hello" => cbor::cbor_map!{"who" => cbor::cbor_text!("tester")} },
    )
    .expect("contract instantiation and call should succeed");
    assert_eq!(
        result,
        cbor::cbor_map! {
            "hello" => cbor::cbor_map!{
                "greeting" => cbor::cbor_text!("hello tester (1)")
            }
        }
    );
}

#[test]
fn test_hello_contract_invalid_request() {
    let code = include_bytes!("../../../../../../tests/contracts/hello/hello.wasm");
    let result = run_contract_with_defaults(
        &code[..],
        1_000_000,
        cbor::cbor_text!("instantiate"),
        cbor::cbor_text!("instantiate"), // This request is invalid.
    )
    .expect_err("contract call should fail");

    assert_eq!(result.module_name(), "contracts.1");
    assert_eq!(result.code(), 1);
    assert_eq!(&result.to_string(), "contract error: bad request");
}

#[test]
fn test_hello_contract_out_of_gas() {
    let code = include_bytes!("../../../../../../tests/contracts/hello/hello.wasm");
    let result = run_contract_with_defaults(
        &code[..],
        1_000,
        cbor::cbor_text!("instantiate"),
        cbor::cbor_map! { "say_hello" => cbor::cbor_map!{"who" => cbor::cbor_text!("tester")} },
    )
    .expect_err("contract call should fail");

    assert_eq!(result.module_name(), "core");
    assert_eq!(result.code(), 12);
    assert_eq!(&result.to_string(), "core: out of gas");
}

#[test]
fn test_bad_contract_infinite_loop_allocate() {
    let code = wat::parse_str(
        r#"
        (module
            (type (;0;) (func))
            (type (;1;) (func (param i32) (result i32)))
            (func (;0;) (type 0))
            (func (;1;) (type 1) (param $p0 i32) (result i32) (loop (br 0)) (i32.const 0))

            (memory $memory (export "memory") 17)
            (export "allocate" (func 1))
            (export "deallocate" (func 0))
            (export "instantiate" (func 0))
            (export "call" (func 0))
        )"#,
    )
    .unwrap();

    let result = run_contract_with_defaults(
        &code[..],
        1_000_000,
        cbor::cbor_text!("instantiate"),
        cbor::cbor_map! { "say_hello" => cbor::cbor_map!{"who" => cbor::cbor_text!("tester")} },
    )
    .expect_err("contract call should fail");

    assert_eq!(result.module_name(), "core");
    assert_eq!(result.code(), 12);
    assert_eq!(&result.to_string(), "core: out of gas");
}

#[test]
fn test_bad_contract_infinite_loop_instantiate() {
    let code = wat::parse_str(
        r#"
        (module
            (type (;0;) (func))
            (type (;1;) (func (param i32) (result i32)))
            (type (;2;) (func (param i32 i32 i32 i32) (result i32 i32)))
            (func (;0;) (type 0))
            (func (;1;) (type 1) (param $p0 i32) (result i32) (i32.const 0))
            (func (;2;) (type 2) (param $p0 i32) (param $p1 i32) (param $p2 i32) (param $p3 i32) (result i32 i32) (loop (br 0)) (i32.const 0) (i32.const 0))

            (memory $memory (export "memory") 17)
            (export "allocate" (func 1))
            (export "deallocate" (func 0))
            (export "instantiate" (func 2))
            (export "call" (func 0))
        )"#,
    ).unwrap();

    let result = run_contract_with_defaults(
        &code[..],
        1_000_000,
        cbor::cbor_text!("instantiate"),
        cbor::cbor_map! { "say_hello" => cbor::cbor_map!{"who" => cbor::cbor_text!("tester")} },
    )
    .expect_err("contract call should fail");

    assert_eq!(result.module_name(), "core");
    assert_eq!(result.code(), 12);
    assert_eq!(&result.to_string(), "core: out of gas");
}

#[test]
fn test_bad_contract_div_by_zero() {
    let code = wat::parse_str(
        r#"
        (module
            (type (;0;) (func))
            (type (;1;) (func (param i32) (result i32)))
            (func (;0;) (type 0))
            (func (;1;) (type 1) (param $p0 i32) (result i32)
                (i32.const 1)
                (i32.const 0)
                (i32.div_s)
            )

            (export "allocate" (func 1))
            (export "deallocate" (func 0))
            (export "instantiate" (func 0))
            (export "call" (func 0))
        )"#,
    )
    .unwrap();

    let result = run_contract_with_defaults(
        &code[..],
        1_000_000,
        cbor::cbor_text!("instantiate"),
        cbor::cbor_map! { "say_hello" => cbor::cbor_map!{"who" => cbor::cbor_text!("tester")} },
    )
    .expect_err("contract call should fail");

    assert_eq!(result.module_name(), "contracts");
    assert_eq!(result.code(), 12);
    assert_eq!(
        &result.to_string(),
        "execution failed: region allocation failed: division by zero"
    );
}

#[test]
fn test_stack_overflow() {
    let code = wat::parse_str(
        r#"
        (module
            (type (;0;) (func))
            (type (;1;) (func (param i32) (result i32)))
            (func (;0;) (type 0))
            (func (;1;) (type 1) (param $p0 i32) (result i32) (i32.const 0) (call 1))

            (export "allocate" (func 1))
            (export "deallocate" (func 0))
            (export "instantiate" (func 0))
            (export "call" (func 0))
        )"#,
    )
    .unwrap();

    let result = run_contract_with_defaults(
        &code[..],
        1_000_000,
        cbor::cbor_text!("instantiate"),
        cbor::cbor_map! { "say_hello" => cbor::cbor_map!{"who" => cbor::cbor_text!("tester")} },
    )
    .expect_err("contract call should fail");

    assert_eq!(result.module_name(), "contracts");
    assert_eq!(result.code(), 12);
    assert_eq!(
        &result.to_string(),
        "execution failed: region allocation failed: stack overflow"
    );
}

#[test]
fn test_memory_grow() {
    let code = wat::parse_str(
        r#"
        (module
            (type (;0;) (func))
            (type (;1;) (func (param i32) (result i32)))
            (func (;0;) (type 0))
            (func (;1;) (type 1) (param $p0 i32) (result i32)
                (loop
                    (memory.grow (i32.const 1))
                    (drop)
                    (br 0)
                )
                (i32.const 0)
            )

            (memory (;0;) 17)
            (export "allocate" (func 1))
            (export "deallocate" (func 0))
            (export "instantiate" (func 0))
            (export "call" (func 0))
        )"#,
    )
    .unwrap();

    let result = run_contract_with_defaults(
        &code[..],
        1_000_000,
        cbor::cbor_text!("instantiate"),
        cbor::cbor_map! { "say_hello" => cbor::cbor_map!{"who" => cbor::cbor_text!("tester")} },
    )
    .expect_err("contract call should fail");

    assert_eq!(result.module_name(), "core");
    assert_eq!(result.code(), 12);
    assert_eq!(&result.to_string(), "core: out of gas");
}
