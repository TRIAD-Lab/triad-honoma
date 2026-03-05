use proptest::prelude::*;
use triad::token::Trd20;
use triad::tvm::state::StateView;

proptest! {
    // Property: sum of balances over used accounts equals total_supply after sequence of mints/transfers
    #[test]
    fn prop_trd20_supply_invariant(num_accounts in 1usize..10, steps in 1usize..200) {
        let state = StateView::new();
        let token = Trd20::new("TRD", 6);

        let mut rng = rand::thread_rng();
        let accounts: Vec<String> = (0..num_accounts).map(|i| format!("a{}", i)).collect();

        for s in 0..steps {
            let pick: u8 = rand::Rng::gen_range(&mut rng, 0..100);
            if pick < 70 {
                // mint
                let idx = rand::Rng::gen_range(&mut rng, 0..num_accounts);
                let to = &accounts[idx];
                let amount: u128 = rand::Rng::gen_range(&mut rng, 1u128..=10_000u128);
                if let Ok(wb) = token.mint(&state, to, amount) { state.put_batch(&wb); }
            } else {
                // transfer
                if num_accounts >= 2 {
                    let from_idx = rand::Rng::gen_range(&mut rng, 0..num_accounts);
                    let mut to_idx = rand::Rng::gen_range(&mut rng, 0..num_accounts);
                    if to_idx == from_idx { to_idx = (to_idx + 1) % num_accounts; }
                    let from = &accounts[from_idx];
                    let to = &accounts[to_idx];
                    let from_bal = token.balance_of(&state, from);
                    if from_bal > 0 {
                        let amount = rand::Rng::gen_range(&mut rng, 1u128..=from_bal);
                        if let Ok(wb) = token.transfer(&state, from, to, amount) { state.put_batch(&wb); }
                    }
                }
            }
        }

        // Verify invariant
        let mut sum: u128 = 0;
        for a in &accounts {
            sum = sum.saturating_add(token.balance_of(&state, a));
        }
        prop_assert_eq!(sum, token.total_supply(&state));
    }
}
