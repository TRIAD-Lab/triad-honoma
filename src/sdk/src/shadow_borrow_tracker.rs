use {
    crate::{instruction::InstructionError},
    std::collections::HashSet,
    std::sync::atomic::{AtomicU64, Ordering},
};

/// Tracks the logical borrow state of accounts across instruction frames.
#[derive(Debug, PartialEq)]
pub struct ShadowBorrowTracker {
    /// A stack of borrow states, one for each instruction frame.
    frames: Vec<BorrowFrame>,
    /// Generation counters for each account in the transaction.
    /// Incremented on realloc to invalidate parent pointers.
    generations: Vec<CloneableAtomicU64>,
}

#[derive(Debug)]
pub struct CloneableAtomicU64(AtomicU64);

impl Clone for CloneableAtomicU64 {
    fn clone(&self) -> Self {
        Self(AtomicU64::new(self.0.load(Ordering::Relaxed)))
    }
}

impl PartialEq for CloneableAtomicU64 {
    fn eq(&self, other: &Self) -> bool {
        self.0.load(Ordering::Relaxed) == other.0.load(Ordering::Relaxed)
    }
}

impl CloneableAtomicU64 {
    pub fn new(val: u64) -> Self {
        Self(AtomicU64::new(val))
    }
}

impl std::ops::Deref for CloneableAtomicU64 {
    type Target = AtomicU64;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Clone for ShadowBorrowTracker {
    fn clone(&self) -> Self {
        Self {
            frames: self.frames.clone(),
            generations: self.generations.clone(), // Vector clone works now
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BorrowFrame {
    writable_borrows: HashSet<u16>,
    readonly_borrows: HashSet<u16>,
}

impl ShadowBorrowTracker {
    pub fn new(num_accounts: usize) -> Self {
        let mut generations = Vec::with_capacity(num_accounts);
        for _ in 0..num_accounts {
            generations.push(CloneableAtomicU64::new(0));
        }
        Self {
            frames: vec![BorrowFrame::default()],
            generations,
        }
    }

    /// Pushes a new instruction frame onto the stack.
    pub fn push_frame(&mut self) {
        self.frames.push(BorrowFrame::default());
    }

    /// Pops the current instruction frame.
    pub fn pop_frame(&mut self) -> Result<(), InstructionError> {
        if self.frames.len() <= 1 {
            return Err(InstructionError::CallDepth);
        }
        self.frames.pop();
        Ok(())
    }

    /// Checks if a borrow is semantically allowed in the current frame.
    /// Returns the current generation of the account.
    pub fn check_and_borrow(
        &mut self,
        index: u16,
        is_writable: bool,
    ) -> Result<u64, InstructionError> {
        let current_frame = self.frames.last_mut().ok_or(InstructionError::CallDepth)?;

        // check if already borrowed in THIS frame.
        if current_frame.writable_borrows.contains(&index) {
            return Err(InstructionError::AccountBorrowFailed);
        }
        if is_writable && current_frame.readonly_borrows.contains(&index) {
            return Err(InstructionError::AccountBorrowFailed);
        }

        // record the borrow in this frame
        if is_writable {
            current_frame.writable_borrows.insert(index);
        } else {
            current_frame.readonly_borrows.insert(index);
        }

        // Return the current generation for tracking invalidation
        Ok(self.generations.get(index as usize)
            .ok_or(InstructionError::MissingAccount)?
            .load(Ordering::Acquire))
    }

    /// Increments the generation of an account, invalidating all existing BorrowedAccount instances.
    pub fn increment_generation(&mut self, index: u16) -> Result<(), InstructionError> {
        let generation = self.generations.get(index as usize).ok_or(InstructionError::MissingAccount)?;
        generation.fetch_add(1, Ordering::Release);
        Ok(())
    }

    /// Validates if a generation is still current.
    pub fn is_valid(&self, index: u16, generation: u64) -> bool {
        self.generations.get(index as usize)
            .map(|g| g.load(Ordering::Acquire) == generation)
            .unwrap_or(false)
    }

    /// Returns a raw pointer to the generation counter for zero-copy VM checks.
    /// SAFETY: generations vector is never resized after creation.
    pub fn get_generation_ptr(&self, index: u16) -> Result<*const u64, InstructionError> {
        let generation = self.generations.get(index as usize).ok_or(InstructionError::MissingAccount)?;
        Ok(&generation.0 as *const AtomicU64 as *const u64)
    }
}
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_shadow_borrow_tracker_frames() {
            let mut tracker = ShadowBorrowTracker::new(10);
            
            // Frame 0: Borrow account 1 as writable
            assert_eq!(tracker.check_and_borrow(1, true).unwrap(), 0);
            
            // Frame 0: Try borrow account 1 again (should fail)
            assert!(tracker.check_and_borrow(1, false).is_err());
            
            // Push Frame 1 (CPI)
            tracker.push_frame();
            
            // Frame 1: Should be able to borrow account 1 (Reset per frame)
            assert_eq!(tracker.check_and_borrow(1, true).unwrap(), 0);
            
            // Pop Frame 1
            tracker.pop_frame().unwrap();
            
            // Back in Frame 0: Still can't borrow account 1
            assert!(tracker.check_and_borrow(1, false).is_err());
        }

        #[test]
        fn test_shadow_borrow_tracker_nested_realloc() {
            let mut tracker = ShadowBorrowTracker::new(10);
            
            // Frame 0: Borrow account 1
            let gen0 = tracker.check_and_borrow(1, true).unwrap();
            assert_eq!(gen0, 0);
            
            // Depth 1 (CPI)
            tracker.push_frame();
            assert_eq!(tracker.check_and_borrow(1, true).unwrap(), 0);
            
            // Depth 2 (CPI)
            tracker.push_frame();
            assert_eq!(tracker.check_and_borrow(1, true).unwrap(), 0);
            
            // Depth 2: Performs REALLOC (increment generation)
            tracker.increment_generation(1).unwrap();
            
            // Depth 2: New borrow should have generation 1
            let gen2_new = tracker.check_and_borrow(1, true).unwrap();
            assert_eq!(gen2_new, 1);
            assert!(tracker.is_valid(1, 1));
            
            // Depth 2: Old generation 0 is now invalid even here
            assert!(!tracker.is_valid(1, gen0));
            
            // Pop Depth 2
            tracker.pop_frame().unwrap();
            
            // Back in Depth 1: BorrowedAccount from before is now invalid
            assert!(!tracker.is_valid(1, gen0));
            
            // Depth 1: New borrow works, gets generation 1
            assert_eq!(tracker.check_and_borrow(1, true).unwrap(), 1);
            
            // Pop Depth 1
            tracker.pop_frame().unwrap();
            
            // Back in Depth 0: Original borrow is invalid
            assert!(!tracker.is_valid(1, gen0));
        }

        #[test]
        fn test_shadow_borrow_tracker_aliasing_audit() {
            let mut tracker = ShadowBorrowTracker::new(10);
            
            // Frame 0: Borrow account 1 as ReadOnly
            tracker.check_and_borrow(1, false).unwrap();
            
            // Frame 0: Try borrow account 1 as Writable (should fail)
            assert!(tracker.check_and_borrow(1, true).is_err());
            
            // Frame 0: Try borrow account 1 as ReadOnly again (should succeed in SVM)
            // Note: In SVM, multiple RO borrows are allowed within the same frame if using same reference,
            // but our tracker is "shadowing" the semantic rules.
            // Actually, SVM RefCell rules: multiple RO or one RW.
            assert!(tracker.check_and_borrow(1, false).is_ok());

            // Push Frame 1
            tracker.push_frame();
            
            // Frame 1: Can borrow account 1 as Writable (Reset per frame)
            assert!(tracker.check_and_borrow(1, true).is_ok());
            
            // Frame 1: Try borrow account 1 as ReadOnly (should fail, already borrowed as Writable in this frame)
            assert!(tracker.check_and_borrow(1, false).is_err());
            
            // Pop Frame 1
            tracker.pop_frame().unwrap();
            
            // Back in Frame 0: Rules from frame 0 still apply
            assert!(tracker.check_and_borrow(1, true).is_err());
            assert!(tracker.check_and_borrow(1, false).is_ok());
        }
    }

