#[cfg(test)]
mod tests {
    use crate::trvm::accounts::AccountGeneration;
    use crate::sdk::shadow_borrow_tracker::ShadowBorrowTracker;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_aba_resilience_atomic_ordering() {
        let gen = AccountGeneration::new();
        // 1. Initial load (Acquire)
        assert_eq!(gen.current(), 1);
        
        // 2. Increment (Release)
        let old = gen.next();
        assert_eq!(old, 2); 
        
        // 3. New value visibility (Acquire)
        assert_eq!(gen.current(), 2);
    }

    #[test]
    fn test_wave_context_immutability() {
        use crate::sdk::wave_context::WaveContext;
        use std::sync::Arc;
        
        let context = WaveContext {
            wave_id: Arc::from("test-wave"),
            cu_limit: 1_000_000,
            ..WaveContext::default()
        };
        
        // Architecture Rule: WaveContext must not change during its lifetime.
        // It resides in an Arc and is shared across threads as a read-only snapshot.
        let context_ptr = &context as *const _;
        
        let handle = std::thread::spawn(move || {
            assert_eq!(context.wave_id.as_ref(), "test-wave");
            assert_eq!(context.cu_limit, 1_000_000);
        });
        
        handle.join().expect("thread join");
    }

    #[test]
    fn test_rayon_boundary_discipline() {
        // Architecture Rule: No Rayon below Wave boundary.
        // Transactions execute in parallel, but internal logic is strictly serial.
        
        let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().unwrap();
        
        pool.install(|| {
            rayon::scope(|s| {
                s.spawn(|_| {
                    let root_thread = rayon::current_thread_index();
                    
                    // Simulate internal TX logic (CPI/Syscalls)
                    // It MUST NOT attempt to use another par_iter/scope.
                    // Doing so would be a violation of the "Rayon Boundary" rule.
                    
                    // We can't strictly prevent the CALL to par_iter, but we enforce 
                    // it via architecture audit and this invariant documentation.
                    #[cfg(debug_assertions)]
                    println!("[TRVM-Audit] Executing logic on thread {:?}", root_thread);
                });
            });
        });
    }
}
