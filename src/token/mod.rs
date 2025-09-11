use crate::tvm::state::{StateView, WriteBatch};
use thiserror::Error;

// TRD-20 state layout (key-value)
// keys (utf-8):
//   trd20/total_supply                   -> u128 LE
//   trd20/balance/<account>              -> u128 LE
//   trd20/allow/<owner>/<spender>        -> u128 LE

const KEY_TOTAL_SUPPLY: &str = "trd20/total_supply";
fn key_balance(acct: &str) -> String { format!("trd20/balance/{}", acct) }

// Backward-compatible alias for existing code paths
pub type TrdToken = Trd20;
fn key_allowance(owner: &str, spender: &str) -> String { format!("trd20/allow/{}/{}", owner, spender) }

#[derive(Debug, Clone)]
pub struct Trd20 {
    pub symbol: String,
    pub decimals: u8,
}

impl Trd20 {
    pub fn new(symbol: impl Into<String>, decimals: u8) -> Self {
        Self { symbol: symbol.into(), decimals }
    }

    pub fn total_supply(&self, state: &StateView) -> u128 {
        read_u128(state.get(KEY_TOTAL_SUPPLY.as_bytes()).as_deref()).unwrap_or(0)
    }

    pub fn balance_of(&self, state: &StateView, owner: &str) -> u128 {
        let k = key_balance(owner);
        read_u128(state.get(k.as_bytes()).as_deref()).unwrap_or(0)
    }

    pub fn allowance(&self, state: &StateView, owner: &str, spender: &str) -> u128 {
        let k = key_allowance(owner, spender);
        read_u128(state.get(k.as_bytes()).as_deref()).unwrap_or(0)
    }

    // Administrative: mint tokens to account (supply increases)
    pub fn mint(&self, state: &StateView, to: &str, amount: u128) -> Result<WriteBatch, TokenError> {
        if amount == 0 { return Err(TokenError::ZeroAmount); }
        let mut wb = WriteBatch::new();
        let supply = self.total_supply(state);
        let new_supply = supply.checked_add(amount).ok_or(TokenError::Overflow)?;
        // Use additive delta to avoid lost updates when merging batches
        wb.add_u128_le_increase(KEY_TOTAL_SUPPLY.as_bytes().to_vec(), amount);

        let bal = self.balance_of(state, to);
        let new_bal = bal.checked_add(amount).ok_or(TokenError::Overflow)?;
        let bk = key_balance(to);
        wb.add_u128_le_increase(bk.into_bytes(), amount);
        Ok(wb)
    }

    // Administrative: burn tokens from account (supply decreases)
    pub fn burn(&self, state: &StateView, from: &str, amount: u128) -> Result<WriteBatch, TokenError> {
        if amount == 0 { return Err(TokenError::ZeroAmount); }
        let mut wb = WriteBatch::new();
        let supply = self.total_supply(state);
        if supply < amount { return Err(TokenError::InsufficientSupply); }
        // Apply negative delta to supply
        wb.add_u128_le_decrease(KEY_TOTAL_SUPPLY.as_bytes().to_vec(), amount);

        let bal = self.balance_of(state, from);
        if bal < amount { return Err(TokenError::InsufficientBalance); }
        let bk = key_balance(from);
        wb.add_u128_le_decrease(bk.into_bytes(), amount);
        Ok(wb)
    }

    pub fn transfer(&self, state: &StateView, from: &str, to: &str, amount: u128) -> Result<WriteBatch, TokenError> {
        if amount == 0 { return Err(TokenError::ZeroAmount); }
        if from == to { return Err(TokenError::SameAddress); }
        let mut wb = WriteBatch::new();
        let from_bal = self.balance_of(state, from);
        if from_bal < amount { return Err(TokenError::InsufficientBalance); }
        let to_bal = self.balance_of(state, to);
        let new_to = to_bal.checked_add(amount).ok_or(TokenError::Overflow)?;
        // Use additive deltas for order-independence
        wb.add_u128_le_decrease(key_balance(from).into_bytes(), amount);
        wb.add_u128_le_increase(key_balance(to).into_bytes(), amount);
        Ok(wb)
    }

    pub fn approve(&self, _state: &StateView, owner: &str, spender: &str, amount: u128) -> Result<WriteBatch, TokenError> {
        if owner == spender { return Err(TokenError::SameAddress); }
        let mut wb = WriteBatch::new();
        wb.put(key_allowance(owner, spender).into_bytes(), amount.to_le_bytes().to_vec());
        Ok(wb)
    }

    pub fn transfer_from(&self, state: &StateView, spender: &str, owner: &str, to: &str, amount: u128) -> Result<WriteBatch, TokenError> {
        if amount == 0 { return Err(TokenError::ZeroAmount); }
        if owner == to { return Err(TokenError::SameAddress); }
        // check allowance
        let allow = self.allowance(state, owner, spender);
        if allow < amount { return Err(TokenError::InsufficientAllowance); }

        let mut wb = WriteBatch::new();
        // update allowance
        wb.add_u128_le_decrease(key_allowance(owner, spender).into_bytes(), amount);

        // move balances
        let owner_bal = self.balance_of(state, owner);
        if owner_bal < amount { return Err(TokenError::InsufficientBalance); }
        let to_bal = self.balance_of(state, to);
        let new_to = to_bal.checked_add(amount).ok_or(TokenError::Overflow)?;
        wb.add_u128_le_decrease(key_balance(owner).into_bytes(), amount);
        wb.add_u128_le_increase(key_balance(to).into_bytes(), amount);
        Ok(wb)
    }
}

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("amount must be > 0")] ZeroAmount,
    #[error("insufficient balance")] InsufficientBalance,
    #[error("insufficient total supply")] InsufficientSupply,
    #[error("insufficient allowance")] InsufficientAllowance,
    #[error("overflow")] Overflow,
    #[error("same address not allowed")] SameAddress,
}

fn read_u128(bytes: Option<&[u8]>) -> Option<u128> {
    let b = bytes?;
    if b.len() < 16 { return None; }
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&b[..16]);
    Some(u128::from_le_bytes(arr))
}
