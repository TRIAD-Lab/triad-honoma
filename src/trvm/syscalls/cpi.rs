use crate::sdk::pubkey::Pubkey;
use crate::sdk::instruction::{Instruction, InstructionError};
use crate::trvm::program_runtime::invoke_context::InvokeContext;
use crate::sdk::stable_layout::stable_instruction::StableInstruction;

/// Native implementation of the `sol_invoke` syscall for TRVM.
/// Provides a high-performance entry point for Cross-Program Invocation (CPI).
pub fn sol_invoke(
    invoke_context: &mut InvokeContext,
    instruction: &Instruction,
    signers: &[Pubkey],
) -> Result<(), InstructionError> {
    // 1. Convert to StableInstruction (TRVM internal format)
    let stable_instruction = StableInstruction::from(instruction.clone());
    
    // 2. Perform Native Invoke
    // This internally calls:
    // - prepare_instruction (uses ShadowBorrowTracker to push a new frame)
    // - process_instruction (recursively executes the program)
    // - pop (cleans up the frame)
    invoke_context.native_invoke(stable_instruction, signers)?;
    
    Ok(())
}
