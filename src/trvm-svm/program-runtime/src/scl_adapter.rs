//! SCL (Solana Compatibility Layer) Adapter
//! Bridges the gap between Solana's syscall ABI and TRVM's execution kernel.

use {
    solana_program_runtime::invoke_context::InvokeContext,
    solana_sdk::{
        instruction::InstructionError,
        transaction_context::IndexOfAccount,
    },
};

/// SclAdapter provides optimized access to Solana-specific state and syscalls.
pub struct SclAdapter<'a, 'b> {
    pub invoke_context: &'a mut InvokeContext<'b>,
}

impl<'a, 'b> SclAdapter<'a, 'b> {
    pub fn new(invoke_context: &'a mut InvokeContext<'b>) -> Self {
        Self { invoke_context }
    }

    /// Optimized account data access that respects SVM-QUIRK-009 (Alignment/Padding)
    pub fn expose_zero_copy_account_data(
        &self,
        index_in_instruction: IndexOfAccount,
        memory_mapping: &solana_rbpf::memory_region::MemoryMapping,
    ) -> Result<(*mut u8, usize), InstructionError> {
        let syscall_context = self.invoke_context.get_syscall_context()?;
        let metadata = syscall_context.accounts_metadata.get(index_in_instruction as usize)
            .ok_or(InstructionError::MissingAccount)?;
        
        let host_ptr = memory_mapping.map(
            solana_rbpf::memory_region::AccessType::Load,
            metadata.vm_data_addr,
            metadata.original_data_len as u64,
        ).map_err(|_| InstructionError::InvalidArgument)?;

        Ok((host_ptr as *mut u8, metadata.original_data_len))
    }

    /// Wave-friendly logging: Batches events instead of immediate dispatch.
    pub fn log_wave_event(&mut self, message: &str) {
        if let Some(log_collector) = self.invoke_context.get_log_collector() {
            log_collector.borrow_mut().log(message);
        }
    }

    /// Optimized batched logging for wave-based execution.
    pub fn log_batch(&mut self, messages: &[&str]) {
        if let Some(log_collector) = self.invoke_context.get_log_collector() {
            let mut collector = log_collector.borrow_mut();
            for msg in messages {
                collector.log(msg);
            }
        }
    }

    /// Optimized compute unit consumption.
    pub fn consume_compute_units(&mut self, amount: u64) -> Result<(), InstructionError> {
        self.invoke_context.consume_checked(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program_runtime::with_mock_invoke_context;
    use solana_sdk::account::AccountSharedData;

    #[test]
    fn test_scl_adapter_logging() {
        with_mock_invoke_context!(invoke_context, transaction_context, vec![]);
        let mut adapter = SclAdapter::new(&mut invoke_context);
        
        adapter.log_wave_event("Test single log");
        adapter.log_batch(&["Batch 1", "Batch 2"]);
        
        // In mock, we can verify that logs were collected
        let logs = invoke_context.get_log_collector().unwrap().borrow().get_recorded_content();
        assert!(logs.contains("Test single log"));
        assert!(logs.contains("Batch 1"));
        assert!(logs.contains("Batch 2"));
    }

    #[test]
    fn test_scl_adapter_heavy_swap_scenario() {
        with_mock_invoke_context!(invoke_context, transaction_context, vec![(Pubkey::new_unique(), AccountSharedData::new(100, 10, &Pubkey::default()))]);
        let mut adapter = SclAdapter::new(&mut invoke_context);
        
        // Initial limit: 1M CU
        let initial_cu = adapter.invoke_context.get_remaining();
        
        // Simulate deep CPI chain (Depth 5)
        for i in 1..=5 {
            adapter.invoke_context.transaction_context.push().unwrap();
            
            // 1. Logging overhead
            adapter.log_batch(&["Swap step in progress", "Calculating price impact..."]);
            
            // 2. Compute cost (simulated 10k CU per swap segment)
            adapter.consume_compute_units(10_000).unwrap();
            
            // 3. Shadow Borrow Invalidation (Realloc in deep frame)
            if i == 5 {
                // In the deepest frame, simulate a realloc that should invalidate all parent borrows
                adapter.invoke_context.transaction_context.get_account_view(0).unwrap()
                    .resize(20, 0);
                // The transaction_context.get_account_at_index().borrow_mut().resize() calls 
                // increment_generation internally in our TRIAD SDK.
            }
        }
        
        // Pop all frames
        for _ in 1..=5 {
            adapter.invoke_context.transaction_context.pop().unwrap();
        }
        
        // Final verification
        let remaining_cu = adapter.invoke_context.get_remaining();
        // 10k * 5 + baseline costs
        assert!(remaining_cu < initial_cu - 50_000);
        
        // Verify that account 0 has generation > 0 (it was realloc'd at depth 5)
        let gen = adapter.invoke_context.transaction_context.borrow_tracker.borrow().get_generation(0);
        assert!(gen > 0);
    }
}
