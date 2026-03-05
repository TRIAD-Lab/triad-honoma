use crate::trvm::scheduler::{ExecutionPhase, WaveContext, WaveExecutionPlan};
use crate::trvm::syscalls::sysvars::SysvarAdapter;
use crate::trvm::program_runtime::sysvar_cache::SysvarCache;
use crate::trvm::program_runtime::message_processor::MessageProcessor;
use crate::sdk::transaction_context::TransactionContext;
use crate::trvm::program_runtime::loaded_programs::LoadedProgramsForTxBatch;
use crate::sdk::feature_set::FeatureSet;
use crate::sdk::hash::Hash;
use rayon::prelude::*;
use std::sync::Arc;
use log::trace;

/// Orchestrates the execution of a Wave using Rayon for maximum parallelism.
pub struct WaveProcessor;

impl WaveProcessor {
    /// Executes a complete Wave according to the provided plan.
    /// Phases are executed sequentially, but transactions within a phase run in parallel.
    pub fn execute_wave(
        plan: &WaveExecutionPlan,
        wave_context: &WaveContext,
        feature_set: Arc<FeatureSet>,
        blockhash: Hash,
    ) -> Result<(), String> {
        // Collect all unique program IDs from all transactions in the wave
        let program_ids: HashSet<u128> = plan.phases.par_iter()
            .flat_map(|phase| phase.transactions.par_iter())
            .collect();
            
        // PRE-LOAD: Load all programs in parallel using Rayon
        let programs_cache = Arc::new(LoadedProgramsForTxBatch::default());
        let program_pubkeys: Vec<Pubkey> = program_ids.iter()
            .map(|id| Pubkey::from(*id))
            .collect();
            
        ProgramManager::preload_programs(&program_pubkeys, &programs_cache);
        
        // EXECUTE: Sequential phases, parallel transactions
        for phase in &plan.phases {
            Self::execute_phase(
                phase,
                wave_context,
                programs_cache.clone(),
                feature_set.clone(),
                blockhash,
            )?;
        }
        Ok(())
    }

    /// Executes a single phase of the wave.
    /// Uses Rayon to execute all phase transactions in parallel.
    pub fn execute_phase(
        phase: &ExecutionPhase,
        wave: &WaveContext,
        programs_cache: Arc<LoadedProgramsForTxBatch>,
        feature_set: Arc<FeatureSet>,
        blockhash: Hash,
    ) -> Result<(), String> {
        phase.transactions.par_iter().try_for_each(|tx_id| {
            let tx_ctx = wave
                .transactions
                .iter()
                .find(|ctx| ctx.tx_id == *tx_id)
                .ok_or_else(|| format!("missing transaction context for {tx_id}"))?;

            // justified clone: WaveContext owns inputs; per-tx context needs owned account vectors
            let mut tx_context = TransactionContext::new(
                tx_ctx.accounts.clone(),
                wave.rent.clone(), // justified clone: rent is small copy per tx
                10, // stack depth — can be tuned by feature_set
                10, // trace depth
            );

            let mut sysvar_cache = SysvarCache::default();
            SysvarAdapter::inject_sysvars(wave, &mut sysvar_cache);

            let mut modified_programs = LoadedProgramsForTxBatch::default();

            let mut invoke_context = crate::trvm::program_runtime::invoke_context::InvokeContext::new_from_wave(
                &mut tx_context,
                wave,
                &sysvar_cache,
                None,
                &programs_cache,
                &mut modified_programs,
                feature_set.clone(),
                blockhash,
                wave.lamports_per_signature,
            );

            #[cfg(debug_assertions)]
            trace!("[TRVM] Rayon thread {:?} executing tx {} in Wave {}", 
                rayon::current_thread_index(), tx_id, wave.wave_id);

            let mut accumulated_consumed_units = 0u64;
            MessageProcessor::process_message(
                &tx_ctx.message,
                &tx_ctx.program_indices,
                &mut tx_context,
                None,
                &programs_cache,
                &mut modified_programs,
                feature_set.clone(),
                wave.blockhash,
                wave.lamports_per_signature,
                &mut accumulated_consumed_units,
            ).map_err(|e| format!("tx {tx_id} failed: {e:?}"))?;

            Ok(())
        })
    }
}
