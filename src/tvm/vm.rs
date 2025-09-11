use crate::tvm::config::TvmConfig;
use crate::tvm::loader::TvmProgram;
use crate::tvm::metrics::StageTimers;
use crate::tvm::state::{KeyAccess, StateView, WriteBatch};
use crate::tvm::host::with_program_context;
use crate::tvm::context::TvmContext;
use crate::tvm::syscalls::SyscallResolver;
use solana_rbpf::{
    aligned_memory::AlignedMemory,
    ebpf,
    error::ProgramResult,
    memory_region::{MemoryMapping, MemoryRegion},
    vm::EbpfVm,
};

#[derive(Debug, Clone, Default)]
pub struct TvmExecutionResult {
    pub ok: bool,
    pub return_code: i64,
    pub logs: Vec<String>,
}

impl std::fmt::Debug for Tvm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tvm")
            .field("cfg", &self.cfg)
            .finish()
    }
}

#[derive(Clone)]
pub struct Tvm {
    pub cfg: TvmConfig,
    pub resolver: SyscallResolver,
}

impl Tvm {
    pub fn new(cfg: TvmConfig, resolver: SyscallResolver) -> Self { Self { cfg, resolver } }

    /// Выполняет eBPF-программу над снапшотом состояния, возвращает результат и WriteBatch для коммита.
    /// Важное: модификация состояния идёт через формирование WriteBatch; прямого доступа к StateView нет.
    pub fn execute(
        &self,
        program: &TvmProgram,
        input: &[u8],
        state: &StateView,
        _access: &KeyAccess,
        network_amp: f64,
    ) -> (TvmExecutionResult, WriteBatch, StageTimers) {
        // Подготовка (snapshot, метрики)
        let mut timers = StageTimers::new();
        let t_prepare = std::time::Instant::now();
        let snapshot = state.snapshot();
        timers.record_prepare(t_prepare.elapsed());

        // Буферы памяти: стек/куча/вход
        let config = program.exec.get_config();
        let sbpf_version = program.exec.get_sbpf_version();
        let mut stack = AlignedMemory::<{ ebpf::HOST_ALIGN }>::zero_filled(config.stack_size());
        let stack_len = stack.len();
        let mut heap = AlignedMemory::<{ ebpf::HOST_ALIGN }>::with_capacity(0);
        let mut input_buf = input.to_vec(); // вход должен быть в writable регионе

        // Регионы памяти
        let mut regions: Vec<MemoryRegion> = vec![
            program.exec.get_ro_region(),
            MemoryRegion::new_writable(stack.as_slice_mut(), ebpf::MM_STACK_START),
            MemoryRegion::new_writable(heap.as_slice_mut(), ebpf::MM_HEAP_START),
            MemoryRegion::new_writable(input_buf.as_mut_slice(), ebpf::MM_INPUT_START),
        ];

        let memory_mapping = match MemoryMapping::new(regions, config, sbpf_version) {
            Ok(m) => m,
            Err(e) => {
                let mut result = TvmExecutionResult { ok: false, return_code: -1, logs: vec![format!("Memory mapping error: {}", e)] };
                return (result, WriteBatch::new(), timers);
            }
        };

        // Контекст и VM
        let loader = program.exec.get_loader().clone();
        let mut context = TvmContext::new(10_000_000);
        let mut vm = EbpfVm::new(loader, &sbpf_version, &mut context, memory_mapping, stack_len);

        // Исполнение
        let t_exec = std::time::Instant::now();
        let mut result = TvmExecutionResult { ok: false, return_code: 0, logs: Vec::new() };
        let mut write_batch = WriteBatch::new();

        let (pr, wb) = with_program_context(snapshot, || {
            vm.execute_program(&program.exec, true).1
        });
        write_batch = wb;

        match pr {
            ProgramResult::Ok(rc) => { result.ok = true; result.return_code = rc as i64; }
            ProgramResult::Err(err) => { result.ok = false; result.return_code = -1; result.logs.push(format!("Program error: {:?}", err)); }
        }
        // Resonance security check: contract must resonate with the network within 5%
        if result.ok {
            let amp = f64::from_bits(result.return_code as u64);
            if !crate::tvm::security::resonate_contract(amp, network_amp) {
                result.ok = false;
                result.logs.push(format!("Resonance check failed: amp={} vs network_amp={} (>=5% deviation)", amp, network_amp));
                write_batch = WriteBatch::new(); // block state changes
            }
        }
        timers.record_execute(t_exec.elapsed());

        let t_commit = std::time::Instant::now();
        timers.record_commit(t_commit.elapsed());

        (result, write_batch, timers)
    }
}
