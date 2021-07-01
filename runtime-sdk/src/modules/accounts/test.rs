//! Tests for the accounts module.
use std::{
    collections::{BTreeMap, BTreeSet},
    iter::FromIterator,
};

use oasis_core_runtime::common::cbor;

use crate::{
    context::{BatchContext, Context},
    module::{AuthHandler, BlockHandler, InvariantHandler},
    modules::core,
    testing::{keys, mock},
    types::{
        token::{BaseUnits, Denomination},
        transaction,
    },
};

use super::{
    types::*, Error, Genesis, Module as Accounts, Parameters, ADDRESS_COMMON_POOL,
    ADDRESS_FEE_ACCUMULATOR, API as _,
};

#[test]
#[should_panic]
fn test_init_incorrect_total_supply_1() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    Accounts::init(
        &mut ctx,
        &Genesis {
            balances: {
                let mut balances = BTreeMap::new();
                // Alice.
                balances.insert(keys::alice::address(), {
                    let mut denominations = BTreeMap::new();
                    denominations.insert(Denomination::NATIVE, 1_000_000.into());
                    denominations
                });
                balances
            },
            ..Default::default()
        },
    );
}

#[test]
#[should_panic]
fn test_init_incorrect_total_supply_2() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    Accounts::init(
        &mut ctx,
        &Genesis {
            balances: {
                let mut balances = BTreeMap::new();
                // Alice.
                balances.insert(keys::alice::address(), {
                    let mut denominations = BTreeMap::new();
                    denominations.insert(Denomination::NATIVE, 1_000_000.into());
                    denominations
                });
                // Bob.
                balances.insert(keys::bob::address(), {
                    let mut denominations = BTreeMap::new();
                    denominations.insert(Denomination::NATIVE, 1_000_000.into());
                    denominations
                });
                balances
            },
            total_supplies: {
                let mut total_supplies = BTreeMap::new();
                total_supplies.insert(Denomination::NATIVE, 1_000_000.into());
                total_supplies
            },
            ..Default::default()
        },
    );
}

#[test]
fn test_init_1() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    Accounts::init(
        &mut ctx,
        &Genesis {
            balances: {
                let mut balances = BTreeMap::new();
                // Alice.
                balances.insert(keys::alice::address(), {
                    let mut denominations = BTreeMap::new();
                    denominations.insert(Denomination::NATIVE, 1_000_000.into());
                    denominations
                });
                balances
            },
            total_supplies: {
                let mut total_supplies = BTreeMap::new();
                total_supplies.insert(Denomination::NATIVE, 1_000_000.into());
                total_supplies
            },
            ..Default::default()
        },
    );
}

#[test]
fn test_init_2() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    Accounts::init(
        &mut ctx,
        &Genesis {
            balances: {
                let mut balances = BTreeMap::new();
                // Alice.
                balances.insert(keys::alice::address(), {
                    let mut denominations = BTreeMap::new();
                    denominations.insert(Denomination::NATIVE, 1_000_000.into());
                    denominations
                });
                // Bob.
                balances.insert(keys::bob::address(), {
                    let mut denominations = BTreeMap::new();
                    denominations.insert(Denomination::NATIVE, 1_000_000.into());
                    denominations
                });
                balances
            },
            total_supplies: {
                let mut total_supplies = BTreeMap::new();
                total_supplies.insert(Denomination::NATIVE, 2_000_000.into());
                total_supplies
            },
            ..Default::default()
        },
    );
}

#[test]
fn test_api_tx_transfer_disabled() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    Accounts::init(
        &mut ctx,
        &Genesis {
            balances: {
                let mut balances = BTreeMap::new();
                // Alice.
                balances.insert(keys::alice::address(), {
                    let mut denominations = BTreeMap::new();
                    denominations.insert(Denomination::NATIVE, 1_000_000.into());
                    denominations
                });
                balances
            },
            total_supplies: {
                let mut total_supplies = BTreeMap::new();
                total_supplies.insert(Denomination::NATIVE, 1_000_000.into());
                total_supplies
            },
            parameters: Parameters {
                transfers_disabled: true,
                gas_costs: Default::default(),
            },
            ..Default::default()
        },
    );

    let tx = transaction::Transaction {
        version: 1,
        call: transaction::Call {
            method: "accounts.Transfer".to_owned(),
            body: cbor::to_value(Transfer {
                to: keys::bob::address(),
                amount: BaseUnits::new(1_000.into(), Denomination::NATIVE),
            }),
        },
        auth_info: transaction::AuthInfo {
            signer_info: vec![transaction::SignerInfo::new(keys::alice::pk(), 0)],
            fee: transaction::Fee {
                amount: Default::default(),
                gas: 1000,
            },
        },
    };

    // Try to transfer.
    ctx.with_tx(tx, |mut tx_ctx, call| {
        assert!(
            matches!(
                Accounts::tx_transfer(&mut tx_ctx, cbor::from_value(call.body).unwrap()),
                Err(Error::Forbidden),
            ),
            "transfers are forbidden",
        )
    });
}

pub(crate) fn init_accounts<C: Context>(ctx: &mut C) {
    Accounts::init(
        ctx,
        &Genesis {
            balances: {
                let mut balances = BTreeMap::new();
                // Alice.
                balances.insert(keys::alice::address(), {
                    let mut denominations = BTreeMap::new();
                    denominations.insert(Denomination::NATIVE, 1_000_000.into());
                    denominations
                });
                balances
            },
            total_supplies: {
                let mut total_supplies = BTreeMap::new();
                total_supplies.insert(Denomination::NATIVE, 1_000_000.into());
                total_supplies
            },
            ..Default::default()
        },
    );
}

#[test]
fn test_api_transfer() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    init_accounts(&mut ctx);

    // Transfer tokens from one account to the other and check balances.
    ctx.with_tx(mock::transaction(), |mut tx_ctx, _call| {
        Accounts::transfer(
            &mut tx_ctx,
            keys::alice::address(),
            keys::bob::address(),
            &BaseUnits::new(1_000.into(), Denomination::NATIVE),
        )
        .expect("transfer should succeed");

        let result = Accounts::transfer(
            &mut tx_ctx,
            keys::alice::address(),
            keys::bob::address(),
            &BaseUnits::new(1_000_000.into(), Denomination::NATIVE),
        );
        assert!(matches!(result, Err(Error::InsufficientBalance)));

        // Check source account balances.
        let bals = Accounts::get_balances(tx_ctx.runtime_state(), keys::alice::address())
            .expect("get_balances should succeed");
        assert_eq!(
            bals.balances[&Denomination::NATIVE],
            999_000.into(),
            "balance in source account should be correct"
        );
        assert_eq!(
            bals.balances.len(),
            1,
            "there should only be one denomination"
        );

        // Check destination account balances.
        let bals = Accounts::get_balances(tx_ctx.runtime_state(), keys::bob::address())
            .expect("get_balances should succeed");
        assert_eq!(
            bals.balances[&Denomination::NATIVE],
            1_000.into(),
            "balance in destination account should be correct"
        );
        assert_eq!(
            bals.balances.len(),
            1,
            "there should only be one denomination"
        );
    });
}

#[test]
fn test_authenticate_tx() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    init_accounts(&mut ctx);

    let mut tx = transaction::Transaction {
        version: 1,
        call: transaction::Call {
            method: "accounts.Transfer".to_owned(),
            body: cbor::to_value(Transfer {
                to: keys::bob::address(),
                amount: BaseUnits::new(1_000.into(), Denomination::NATIVE),
            }),
        },
        auth_info: transaction::AuthInfo {
            signer_info: vec![transaction::SignerInfo::new(keys::alice::pk(), 0)],
            fee: transaction::Fee {
                amount: BaseUnits::new(1_000.into(), Denomination::NATIVE),
                gas: 1000,
            },
        },
    };

    // Should succeed with enough funds to pay for fees.
    Accounts::authenticate_tx(&mut ctx, &tx).expect("transaction authentication should succeed");
    // Check source account balances.
    let bals = Accounts::get_balances(ctx.runtime_state(), keys::alice::address())
        .expect("get_balances should succeed");
    assert_eq!(
        bals.balances[&Denomination::NATIVE],
        999_000.into(),
        "fees should be subtracted from source account"
    );
    assert_eq!(
        bals.balances.len(),
        1,
        "there should only be one denomination"
    );
    // Check source account nonce.
    let nonce = Accounts::get_nonce(ctx.runtime_state(), keys::alice::address())
        .expect("get_nonce should succeed");
    assert_eq!(nonce, 1, "nonce should be incremented");

    // Should fail with an invalid nonce.
    let result = Accounts::authenticate_tx(&mut ctx, &tx);
    assert!(matches!(result, Err(core::Error::InvalidNonce)));

    // Should fail when there's not enough balance to pay fees.
    tx.auth_info.signer_info[0].nonce = nonce;
    tx.auth_info.fee.amount = BaseUnits::new(1_100_000.into(), Denomination::NATIVE);
    let result = Accounts::authenticate_tx(&mut ctx, &tx);
    assert!(matches!(result, Err(core::Error::InsufficientFeeBalance)));
}

#[test]
fn test_tx_transfer() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    init_accounts(&mut ctx);

    let tx = transaction::Transaction {
        version: 1,
        call: transaction::Call {
            method: "accounts.Transfer".to_owned(),
            body: cbor::to_value(Transfer {
                to: keys::bob::address(),
                amount: BaseUnits::new(1_000.into(), Denomination::NATIVE),
            }),
        },
        auth_info: transaction::AuthInfo {
            signer_info: vec![transaction::SignerInfo::new(keys::alice::pk(), 0)],
            fee: transaction::Fee {
                amount: Default::default(),
                gas: 1000,
            },
        },
    };

    // Transfer tokens from one account to the other and check balances.
    ctx.with_tx(tx, |mut tx_ctx, call| {
        Accounts::tx_transfer(&mut tx_ctx, cbor::from_value(call.body).unwrap())
            .expect("transfer should succeed");

        // Check source account balances.
        let bals = Accounts::get_balances(tx_ctx.runtime_state(), keys::alice::address())
            .expect("get_balances should succeed");
        assert_eq!(
            bals.balances[&Denomination::NATIVE],
            999_000.into(),
            "balance in source account should be correct"
        );
        assert_eq!(
            bals.balances.len(),
            1,
            "there should only be one denomination"
        );

        // Check destination account balances.
        let bals = Accounts::get_balances(tx_ctx.runtime_state(), keys::bob::address())
            .expect("get_balances should succeed");
        assert_eq!(
            bals.balances[&Denomination::NATIVE],
            1_000.into(),
            "balance in destination account should be correct"
        );
        assert_eq!(
            bals.balances.len(),
            1,
            "there should only be one denomination"
        );
    });
}

#[test]
fn test_fee_disbursement() {
    let mut mock = mock::Mock::default();

    // Configure some good entities so they get the fees.
    mock.runtime_round_results.good_compute_entities = vec![
        keys::bob::pk_ed25519().into(),
        keys::charlie::pk_ed25519().into(),
    ];

    let mut ctx = mock.create_ctx();

    init_accounts(&mut ctx);

    let tx = transaction::Transaction {
        version: 1,
        call: transaction::Call {
            method: "accounts.Transfer".to_owned(),
            body: cbor::to_value(Transfer {
                to: keys::bob::address(),
                amount: Default::default(),
            }),
        },
        auth_info: transaction::AuthInfo {
            signer_info: vec![transaction::SignerInfo::new(keys::alice::pk(), 0)],
            fee: transaction::Fee {
                // Use an amount that does not split nicely among the good compute entities.
                amount: BaseUnits::new(1_001.into(), Denomination::NATIVE),
                gas: 1000,
            },
        },
    };

    // Authenticate transaction, fees should be moved to accumulator.
    Accounts::authenticate_tx(&mut ctx, &tx).expect("transaction authentication should succeed");
    // Run end block handler.
    Accounts::end_block(&mut ctx);

    // Check source account balances.
    let bals = Accounts::get_balances(ctx.runtime_state(), keys::alice::address())
        .expect("get_balances should succeed");
    assert_eq!(
        bals.balances[&Denomination::NATIVE],
        998_999.into(),
        "fees should be subtracted from source account"
    );
    // Nothing should be disbursed yet to good compute entity accounts.
    let bals = Accounts::get_balances(ctx.runtime_state(), keys::bob::address())
        .expect("get_balances should succeed");
    assert!(bals.balances.is_empty(), "nothing should be disbursed yet");
    // Check second good compute entity account balances.
    let bals = Accounts::get_balances(ctx.runtime_state(), keys::charlie::address())
        .expect("get_balances should succeed");
    assert!(bals.balances.is_empty(), "nothing should be disbursed yet");

    // Fees should be placed in the fee accumulator address to be disbursed in the next round.
    let bals = Accounts::get_balances(ctx.runtime_state(), *ADDRESS_FEE_ACCUMULATOR)
        .expect("get_balances should succeed");
    assert_eq!(
        bals.balances[&Denomination::NATIVE],
        1001.into(),
        "fees should be held in the fee accumulator address"
    );

    // Simulate another block happening.
    Accounts::end_block(&mut ctx);

    // Check first good compute entity account balances.
    let bals = Accounts::get_balances(ctx.runtime_state(), keys::bob::address())
        .expect("get_balances should succeed");
    assert_eq!(
        bals.balances[&Denomination::NATIVE],
        500.into(),
        "fees should be disbursed to good compute entity"
    );
    // Check second good compute entity account balances.
    let bals = Accounts::get_balances(ctx.runtime_state(), keys::charlie::address())
        .expect("get_balances should succeed");
    assert_eq!(
        bals.balances[&Denomination::NATIVE],
        500.into(),
        "fees should be disbursed to good compute entity"
    );

    // Check the common pool which should have the remainder.
    let bals = Accounts::get_balances(ctx.runtime_state(), *ADDRESS_COMMON_POOL)
        .expect("get_balances should succeed");
    assert_eq!(
        bals.balances[&Denomination::NATIVE],
        1.into(),
        "remainder should be disbursed to the common pool"
    );
}

#[test]
fn test_query_list_accounts() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    let accs = Accounts::query_list_accounts(&mut ctx).expect("query accounts should succeed");
    assert_eq!(accs.len(), 0, "there should be no accounts initially");

    let dn = Denomination::NATIVE;
    let d1: Denomination = "den1".parse().unwrap();
    let gen = Genesis {
        balances: {
            let mut balances = BTreeMap::new();
            // Alice.
            balances.insert(keys::alice::address(), {
                let mut denominations = BTreeMap::new();
                denominations.insert(dn.clone(), 1_000_000.into());
                denominations.insert(d1.clone(), 1_000.into());
                denominations
            });
            // Bob.
            balances.insert(keys::bob::address(), {
                let mut denominations = BTreeMap::new();
                denominations.insert(d1.clone(), 2_000.into());
                denominations
            });
            balances
        },
        total_supplies: {
            let mut total_supplies = BTreeMap::new();
            total_supplies.insert(dn.clone(), 1_000_000.into());
            total_supplies.insert(d1.clone(), 3_000.into());
            total_supplies
        },
        ..Default::default()
    };
    Accounts::init(&mut ctx, &gen);

    ctx.with_tx(mock::transaction(), |mut tx_ctx, _call| {
        let accs =
            Accounts::query_list_accounts(&mut tx_ctx).expect("query accounts should succeed");
        assert_eq!(accs.len(), 2, "there should be two accounts");
        assert_eq!(
            accs,
            BTreeSet::from_iter([keys::alice::address(), keys::bob::address()]),
            "list accounts addresses should be correct"
        );
    });
}

#[test]
fn test_get_all_balances_and_total_supplies_basic() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    let alice = keys::alice::address();
    let bob = keys::bob::address();

    let gen = Genesis {
        balances: {
            let mut balances = BTreeMap::new();
            // Alice.
            balances.insert(alice, {
                let mut denominations = BTreeMap::new();
                denominations.insert(Denomination::NATIVE, 1_000_000.into());
                denominations
            });
            // Bob.
            balances.insert(bob, {
                let mut denominations = BTreeMap::new();
                denominations.insert(Denomination::NATIVE, 2_000_000.into());
                denominations
            });
            balances
        },
        total_supplies: {
            let mut total_supplies = BTreeMap::new();
            total_supplies.insert(Denomination::NATIVE, 3_000_000.into());
            total_supplies
        },
        ..Default::default()
    };

    Accounts::init(&mut ctx, &gen);

    let all_bals =
        Accounts::get_all_balances(ctx.runtime_state()).expect("get_all_balances should succeed");
    for (addr, bals) in &all_bals {
        assert_eq!(bals.len(), 1, "exactly one denomination should be present");
        assert!(
            bals.contains_key(&Denomination::NATIVE),
            "only native denomination should be present"
        );
        if addr == &alice {
            assert_eq!(
                bals[&Denomination::NATIVE],
                1_000_000.into(),
                "Alice's balance should be 1000000"
            );
        } else if addr == &bob {
            assert_eq!(
                bals[&Denomination::NATIVE],
                2_000_000.into(),
                "Bob's balance should be 2000000"
            );
        } else {
            panic!("invalid address");
        }
    }

    let ts = Accounts::get_total_supplies(ctx.runtime_state())
        .expect("get_total_supplies should succeed");
    assert_eq!(
        ts.len(),
        1,
        "exactly one denomination should be present in total supplies"
    );
    assert!(
        ts.contains_key(&Denomination::NATIVE),
        "only native denomination should be present in total supplies"
    );
    assert_eq!(
        ts[&Denomination::NATIVE],
        3_000_000.into(),
        "total supply should be 3000000"
    );
}

#[test]
fn test_get_all_balances_and_total_supplies_more() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    let dn = Denomination::NATIVE;
    let d1: Denomination = "den1".parse().unwrap();
    let d2: Denomination = "den2".parse().unwrap();
    let d3: Denomination = "den3".parse().unwrap();

    let alice = keys::alice::address();
    let bob = keys::bob::address();

    let gen = Genesis {
        balances: {
            let mut balances = BTreeMap::new();
            // Alice.
            balances.insert(alice, {
                let mut denominations = BTreeMap::new();
                denominations.insert(dn.clone(), 1_000_000.into());
                denominations.insert(d1.clone(), 1_000.into());
                denominations.insert(d2.clone(), 100.into());
                denominations
            });
            // Bob.
            balances.insert(bob, {
                let mut denominations = BTreeMap::new();
                denominations.insert(d1.clone(), 2_000.into());
                denominations.insert(d3.clone(), 200.into());
                denominations
            });
            balances
        },
        total_supplies: {
            let mut total_supplies = BTreeMap::new();
            total_supplies.insert(dn.clone(), 1_000_000.into());
            total_supplies.insert(d1.clone(), 3_000.into());
            total_supplies.insert(d2.clone(), 100.into());
            total_supplies.insert(d3.clone(), 200.into());
            total_supplies
        },
        ..Default::default()
    };

    Accounts::init(&mut ctx, &gen);

    let all_bals =
        Accounts::get_all_balances(ctx.runtime_state()).expect("get_all_balances should succeed");
    for (addr, bals) in &all_bals {
        if addr == &alice {
            assert_eq!(bals.len(), 3, "Alice should have exactly 3 denominations");
            assert_eq!(
                bals[&dn],
                1_000_000.into(),
                "Alice's native balance should be 1000000"
            );
            assert_eq!(
                bals[&d1],
                1_000.into(),
                "Alice's den1 balance should be 1000"
            );
            assert_eq!(bals[&d2], 100.into(), "Alice's den2 balance should be 100");
        } else if addr == &bob {
            assert_eq!(bals.len(), 2, "Bob should have exactly 2 denominations");
            assert_eq!(bals[&d1], 2_000.into(), "Bob's den1 balance should be 2000");
            assert_eq!(bals[&d3], 200.into(), "Bob's den3 balance should be 200");
        } else {
            panic!("invalid address");
        }
    }

    let ts = Accounts::get_total_supplies(ctx.runtime_state())
        .expect("get_total_supplies should succeed");
    assert_eq!(
        ts.len(),
        4,
        "exactly 4 denominations should be present in total supplies"
    );
    assert!(
        ts.contains_key(&dn),
        "native denomination should be present in total supplies"
    );
    assert!(
        ts.contains_key(&d1),
        "den1 denomination should be present in total supplies"
    );
    assert!(
        ts.contains_key(&d2),
        "den2 denomination should be present in total supplies"
    );
    assert!(
        ts.contains_key(&d3),
        "den3 denomination should be present in total supplies"
    );
    assert_eq!(
        ts[&dn],
        1_000_000.into(),
        "native total supply should be 1000000"
    );
    assert_eq!(ts[&d1], 3_000.into(), "den1 total supply should be 3000");
    assert_eq!(ts[&d2], 100.into(), "den2 total supply should be 100");
    assert_eq!(ts[&d3], 200.into(), "den3 total supply should be 200");
}

#[test]
fn test_check_invariants_basic() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    init_accounts(&mut ctx);

    assert!(
        Accounts::check_invariants(&mut ctx).is_ok(),
        "invariants check should succeed"
    );
}

#[test]
fn test_check_invariants_more() {
    let mut mock = mock::Mock::default();
    let mut ctx = mock.create_ctx();

    let dn = Denomination::NATIVE;
    let d1: Denomination = "den1".parse().unwrap();
    let d2: Denomination = "den2".parse().unwrap();
    let d3: Denomination = "den3".parse().unwrap();

    let alice = keys::alice::address();
    let bob = keys::bob::address();
    let charlie = keys::charlie::address();

    let gen = Genesis {
        balances: {
            let mut balances = BTreeMap::new();
            // Alice.
            balances.insert(alice, {
                let mut denominations = BTreeMap::new();
                denominations.insert(dn.clone(), 1_000_000.into());
                denominations.insert(d1.clone(), 1_000.into());
                denominations.insert(d2.clone(), 100.into());
                denominations
            });
            // Bob.
            balances.insert(bob, {
                let mut denominations = BTreeMap::new();
                denominations.insert(d1.clone(), 2_000.into());
                denominations.insert(d3.clone(), 200.into());
                denominations
            });
            balances
        },
        total_supplies: {
            let mut total_supplies = BTreeMap::new();
            total_supplies.insert(dn.clone(), 1_000_000.into());
            total_supplies.insert(d1.clone(), 3_000.into());
            total_supplies.insert(d2.clone(), 100.into());
            total_supplies.insert(d3.clone(), 200.into());
            total_supplies
        },
        ..Default::default()
    };

    Accounts::init(&mut ctx, &gen);
    assert!(
        Accounts::check_invariants(&mut ctx).is_ok(),
        "initial inv chk should succeed"
    );

    assert!(
        Accounts::add_amount(
            ctx.runtime_state(),
            charlie,
            &BaseUnits::new(100.into(), d1.clone())
        )
        .is_ok(),
        "giving Charlie money should succeed"
    );
    assert!(
        Accounts::check_invariants(&mut ctx).is_err(),
        "inv chk 1 should fail"
    );

    assert!(
        Accounts::inc_total_supply(ctx.runtime_state(), &BaseUnits::new(100.into(), d1.clone()))
            .is_ok(),
        "increasing total supply should succeed"
    );
    assert!(
        Accounts::check_invariants(&mut ctx).is_ok(),
        "inv chk 2 should succeed"
    );

    let d4: Denomination = "den4".parse().unwrap();

    assert!(
        Accounts::add_amount(
            ctx.runtime_state(),
            charlie,
            &BaseUnits::new(300.into(), d4.clone())
        )
        .is_ok(),
        "giving Charlie more money should succeed"
    );
    assert!(
        Accounts::check_invariants(&mut ctx).is_err(),
        "inv chk 3 should fail"
    );

    assert!(
        Accounts::inc_total_supply(ctx.runtime_state(), &BaseUnits::new(300.into(), d4.clone()))
            .is_ok(),
        "increasing total supply should succeed"
    );
    assert!(
        Accounts::check_invariants(&mut ctx).is_ok(),
        "inv chk 4 should succeed"
    );

    let d5: Denomination = "den5".parse().unwrap();

    assert!(
        Accounts::inc_total_supply(ctx.runtime_state(), &BaseUnits::new(123.into(), d5.clone()))
            .is_ok(),
        "increasing total supply should succeed"
    );
    assert!(
        Accounts::check_invariants(&mut ctx).is_err(),
        "inv chk 5 should fail"
    );

    assert!(
        Accounts::add_amount(
            ctx.runtime_state(),
            charlie,
            &BaseUnits::new(123.into(), d5.clone())
        )
        .is_ok(),
        "giving Charlie more money should succeed"
    );
    assert!(
        Accounts::check_invariants(&mut ctx).is_ok(),
        "inv chk 6 should succeed"
    );
}
