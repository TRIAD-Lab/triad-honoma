use serde_json::Value;
use crate::tvm::{TvmExecutionResult};
use crate::tvm::state::{StateView, WriteBatch};

mod trd20;

/// Try to execute a precompile based on the provided input bytes.
/// Returns Some((result, write_batch)) if recognized and executed, otherwise None.
pub fn try_execute_precompile(input: &[u8], state: &StateView, network_amp: f64) -> Option<(TvmExecutionResult, WriteBatch)> {
    // Heuristic: expect UTF-8 JSON
    let s = std::str::from_utf8(input).ok()?;
    let v: Value = serde_json::from_str(s).ok()?;
    match v.get("contract").and_then(|x| x.as_str()) {
        Some("TRD20") => trd20::execute(&v, state, network_amp),
        _ => None,
    }
}
