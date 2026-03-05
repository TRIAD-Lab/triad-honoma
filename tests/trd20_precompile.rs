use triad::token::Trd20;
use triad::tvm::precompile::try_execute_precompile;
use triad::tvm::state::StateView;

fn json_bytes(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

#[test]
fn precompile_mint_and_transfer() {
    let state = StateView::new();
    // Mint 1000 to alice
    let input = r#"{
        "contract":"TRD20",
        "method":"mint",
        "symbol":"TRD",
        "decimals":6,
        "to":"alice",
        "amount":"1000"
    }"#;
    let (res, wb) =
        try_execute_precompile(&json_bytes(input), &state, 1.0).expect("precompile detected");
    assert!(res.ok, "mint should be ok, logs: {:?}", res.logs);
    state.put_batch(&wb);

    // Transfer 600 from alice to bob
    let tx = r#"{
        "contract":"TRD20",
        "method":"transfer",
        "from":"alice",
        "to":"bob",
        "amount":600
    }"#;
    let (res, wb) =
        try_execute_precompile(&json_bytes(tx), &state, 1.0).expect("precompile detected");
    assert!(res.ok, "transfer should be ok, logs: {:?}", res.logs);
    state.put_batch(&wb);

    // Verify via token API
    let token = Trd20::new("TRD", 6);
    assert_eq!(token.total_supply(&state), 1000);
    assert_eq!(token.balance_of(&state, "alice"), 400);
    assert_eq!(token.balance_of(&state, "bob"), 600);
}

#[test]
fn precompile_overflow_guard_mint() {
    let state = StateView::new();
    // First mint MAX to alice
    let m1 = format!(
        "{{\"contract\":\"TRD20\",\"method\":\"mint\",\"to\":\"alice\",\"amount\":\"{}\"}}",
        u128::MAX
    );
    let (res, wb) =
        try_execute_precompile(&json_bytes(&m1), &state, 0.5).expect("precompile detected");
    assert!(res.ok);
    state.put_batch(&wb);

    // Second mint must fail with overflow
    let m2 = r#"{"contract":"TRD20","method":"mint","to":"bob","amount":1}"#;
    let (res, wb) =
        try_execute_precompile(&json_bytes(m2), &state, 0.5).expect("precompile detected");
    assert!(!res.ok, "expected overflow error, logs: {:?}", res.logs);
    assert!(wb.is_empty(), "no writes on error");
}
