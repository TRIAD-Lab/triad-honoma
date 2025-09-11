// TVM security checks related to resonance validation
// All comments and logs should be in English per project rules.

/// Returns true if the wave (contract) resonates with the network wave
/// according to the 5% deviation threshold rule.
#[inline]
pub fn resonate_contract(wave_amplitude: f64, network_amplitude: f64) -> bool {
    let denom = if network_amplitude.abs() < 1e-12 { 1.0 } else { network_amplitude.abs() };
    let diff = ((wave_amplitude - network_amplitude).abs() / denom).min(1.0);
    diff < 0.05
}
