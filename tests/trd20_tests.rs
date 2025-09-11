use triad::tvm::state::StateView;
use triad::token::{Trd20, TokenError};

fn u128_le(bytes: &[u8]) -> u128 { // helper if needed later
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&bytes[..16]);
    u128::from_le_bytes(arr)
}

#[test]
fn trd20_concurrent_transfers_no_net_change() {
    use std::thread;
    let state = StateView::new();
    let token = Trd20::new("TRD", 6);

    // Prepare N accounts with equal balance
    let n = 64usize;
    for i in 0..n {
        let acct = format!("a{}", i);
        let wb = token.mint(&state, &acct, 1_000).unwrap();
        state.put_batch(&wb);
    }

    // Spawn N threads each transferring 1 to next account (ring)
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        let state_cl = state.clone();
        let token_cl = token.clone();
        let from = format!("a{}", i);
        let to = format!("a{}", (i + 1) % n);
        handles.push(thread::spawn(move || {
            token_cl.transfer(&state_cl, &from, &to, 1).expect("transfer ok")
        }));
    }

    // Collect write batches and apply sequentially
    for h in handles {
        let wb = h.join().expect("thread ok");
        state.put_batch(&wb);
    }

    // Validate invariants: each account unchanged, total supply preserved
    for i in 0..n {
        let acct = format!("a{}", i);
        assert_eq!(token.balance_of(&state, &acct), 1_000);
    }
    assert_eq!(token.total_supply(&state), (n as u128) * 1_000);
}

#[test]
fn trd20_overflow_guard_mint() {
    let state = StateView::new();
    let token = Trd20::new("TRD", 6);

    // Mint u128::MAX once is allowed
    let wb = token.mint(&state, "alice", u128::MAX).expect("first mint ok");
    state.put_batch(&wb);
    assert_eq!(token.total_supply(&state), u128::MAX);
    assert_eq!(token.balance_of(&state, "alice"), u128::MAX);

    // Next mint must overflow total supply
    let err = token.mint(&state, "bob", 1).expect_err("overflow expected");
    match err { TokenError::Overflow => {}, _ => panic!("wrong error: {:?}", err) }
}

#[test]
fn trd20_zero_allowance() {
    let state = StateView::new();
    let token = Trd20::new("TRD", 6);

    let wb = token.mint(&state, "owner", 10).unwrap();
    state.put_batch(&wb);

    // Approve zero
    let wb = token.approve(&state, "owner", "sp", 0).unwrap();
    state.put_batch(&wb);
    assert_eq!(token.allowance(&state, "owner", "sp"), 0);

    // transfer_from with zero must be rejected as ZeroAmount
    let err = token.transfer_from(&state, "sp", "owner", "bob", 0).expect_err("zero amount expected");
    match err { TokenError::ZeroAmount => {}, _ => panic!("wrong error: {:?}", err) }
}

#[test]
fn trd20_mint_and_balances() {
    let state = StateView::new();
    let token = Trd20::new("TRD", 6);

    // Mint 1000 to Alice
    let wb = token.mint(&state, "alice", 1_000).expect("mint should succeed");
    state.put_batch(&wb);

    assert_eq!(token.total_supply(&state), 1_000);
    assert_eq!(token.balance_of(&state, "alice"), 1_000);
}

#[test]
fn trd20_transfer() {
    let state = StateView::new();
    let token = Trd20::new("TRD", 6);

    // Seed Alice with 1000
    let wb = token.mint(&state, "alice", 1_000).unwrap();
    state.put_batch(&wb);

    // Transfer 300 to Bob
    let wb = token.transfer(&state, "alice", "bob", 300).expect("transfer should succeed");
    state.put_batch(&wb);

    assert_eq!(token.balance_of(&state, "alice"), 700);
    assert_eq!(token.balance_of(&state, "bob"), 300);
    assert_eq!(token.total_supply(&state), 1_000);
}

#[test]
fn trd20_burn() {
    let state = StateView::new();
    let token = Trd20::new("TRD", 6);

    // Seed Alice with 1000
    let wb = token.mint(&state, "alice", 1_000).unwrap();
    state.put_batch(&wb);

    // Burn 200 from Alice
    let wb = token.burn(&state, "alice", 200).expect("burn should succeed");
    state.put_batch(&wb);

    assert_eq!(token.balance_of(&state, "alice"), 800);
    assert_eq!(token.total_supply(&state), 800);
}

#[test]
fn trd20_approve_and_transfer_from() {
    let state = StateView::new();
    let token = Trd20::new("TRD", 6);

    // Seed Owner with 1000
    let wb = token.mint(&state, "owner", 1_000).unwrap();
    state.put_batch(&wb);

    // Approve spender for 400
    let wb = token.approve(&state, "owner", "sp", 400).expect("approve should succeed");
    state.put_batch(&wb);
    assert_eq!(token.allowance(&state, "owner", "sp"), 400);

    // Spender transfers 250 from owner to Bob
    let wb = token
        .transfer_from(&state, "sp", "owner", "bob", 250)
        .expect("transfer_from should succeed");
    state.put_batch(&wb);

    assert_eq!(token.balance_of(&state, "owner"), 750);
    assert_eq!(token.balance_of(&state, "bob"), 250);
    assert_eq!(token.allowance(&state, "owner", "sp"), 150);
    assert_eq!(token.total_supply(&state), 1_000);
}

#[test]
fn trd20_error_paths() {
    let state = StateView::new();
    let token = Trd20::new("TRD", 6);

    // Zero amount
    assert!(matches!(token.mint(&state, "alice", 0), Err(TokenError::ZeroAmount)));
    assert!(matches!(token.transfer(&state, "a", "b", 0), Err(TokenError::ZeroAmount)));

    // Same address not allowed
    assert!(matches!(token.transfer(&state, "a", "a", 10), Err(TokenError::SameAddress)));

    // Insufficient balance
    assert!(matches!(token.transfer(&state, "alice", "bob", 1), Err(TokenError::InsufficientBalance)));

    // Approve then try over-allowance
    let wb = token.mint(&state, "owner", 100).unwrap();
    state.put_batch(&wb);
    let wb = token.approve(&state, "owner", "sp", 30).unwrap();
    state.put_batch(&wb);
    assert!(matches!(
        token.transfer_from(&state, "sp", "owner", "bob", 31),
        Err(TokenError::InsufficientAllowance)
    ));
}
