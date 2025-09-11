use serde_json::Value;
use crate::tvm::{TvmExecutionResult};
use crate::tvm::state::{StateView, WriteBatch};
use crate::token::Trd20;

fn parse_u128(v: &Value) -> Option<u128> {
    if let Some(n) = v.as_u64() { return Some(n as u128); }
    if let Some(s) = v.as_str() { return s.parse::<u128>().ok(); }
    None
}

pub fn execute(v: &Value, state: &StateView, network_amp: f64) -> Option<(TvmExecutionResult, WriteBatch)> {
    let method = v.get("method")?.as_str()?;
    let symbol = v.get("symbol").and_then(|x| x.as_str()).unwrap_or("TRD");
    let decimals = v.get("decimals").and_then(|x| x.as_u64()).unwrap_or(6) as u8;
    let token = Trd20::new(symbol, decimals);

    let mut logs = Vec::new();
    let wb_res = match method {
        "mint" => {
            let to = v.get("to")?.as_str()?;
            let amount = parse_u128(v.get("amount" )?)?;
            logs.push(format!("TRD20::mint to={} amount={}", to, amount));
            token.mint(state, to, amount)
        }
        "burn" => {
            let from = v.get("from")?.as_str()?;
            let amount = parse_u128(v.get("amount")?)?;
            logs.push(format!("TRD20::burn from={} amount={}", from, amount));
            token.burn(state, from, amount)
        }
        "transfer" => {
            let from = v.get("from")?.as_str()?;
            let to = v.get("to")?.as_str()?;
            let amount = parse_u128(v.get("amount")?)?;
            logs.push(format!("TRD20::transfer from={} to={} amount={}", from, to, amount));
            token.transfer(state, from, to, amount)
        }
        "approve" => {
            let owner = v.get("owner")?.as_str()?;
            let spender = v.get("spender")?.as_str()?;
            let amount = parse_u128(v.get("amount")?)?;
            logs.push(format!("TRD20::approve owner={} spender={} amount={}", owner, spender, amount));
            token.approve(state, owner, spender, amount)
        }
        "transfer_from" => {
            let spender = v.get("spender")?.as_str()?;
            let owner = v.get("owner")?.as_str()?;
            let to = v.get("to")?.as_str()?;
            let amount = parse_u128(v.get("amount")?)?;
            logs.push(format!("TRD20::transfer_from spender={} owner={} to={} amount={}", spender, owner, to, amount));
            token.transfer_from(state, spender, owner, to, amount)
        }
        _ => { return None; }
    };

    match wb_res {
        Ok(wb) => {
            let amp_bits = f64::to_bits(network_amp);
            let res = TvmExecutionResult { ok: true, return_code: amp_bits as i64, logs };
            Some((res, wb))
        }
        Err(e) => {
            let mut logs = logs;
            logs.push(format!("TRD20 error: {}", e));
            let res = TvmExecutionResult { ok: false, return_code: -1, logs };
            Some((res, WriteBatch::new()))
        }
    }
}
