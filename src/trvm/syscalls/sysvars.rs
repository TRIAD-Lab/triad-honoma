use crate::trvm::scheduler::WaveContext;
use crate::trvm::program_runtime::sysvar_cache::SysvarCache;

/// Adapts WaveContext invariants to be visible within InvokeContext.
pub struct SysvarAdapter;

impl SysvarAdapter {
    /// Injects wave-level sysvars into the sysvar cache used by syscalls.
    pub fn inject_sysvars(wave: &WaveContext, sysvar_cache: &mut SysvarCache) {
        // justified clone: sysvar cache stores owned copies per transaction
        sysvar_cache.set_clock(wave.clock.clone());
        sysvar_cache.set_rent(wave.rent.clone());
        sysvar_cache.set_epoch_schedule(wave.epoch_schedule.clone());
        sysvar_cache.set_slot_hashes(wave.slot_hashes.clone());
    }
}
