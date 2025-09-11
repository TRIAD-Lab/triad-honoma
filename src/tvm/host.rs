use std::cell::RefCell;
use crate::tvm::state::{Snapshot, WriteBatch, Value, Key};

thread_local! {
    static CTX: RefCell<Option<ProgramContext>> = RefCell::new(None);
}

#[derive(Debug, Default)]
pub struct ProgramContext {
    pub snapshot: Snapshot,
    pub batch: WriteBatch,
    pub scratch: Vec<u8>,
}

impl ProgramContext {
    pub fn new(snapshot: Snapshot) -> Self { Self { snapshot, batch: WriteBatch::new(), scratch: Vec::new() } }
}

pub fn with_program_context<F, T>(snapshot: Snapshot, f: F) -> (T, WriteBatch)
where F: FnOnce() -> T {
    CTX.with(|cell| {
        *cell.borrow_mut() = Some(ProgramContext::new(snapshot));
    });
    let out = f();
    let batch = CTX.with(|cell| {
        let mut opt = cell.borrow_mut();
        let ctx = opt.take().unwrap_or_default();
        ctx.batch
    });
    (out, batch)
}

// ===== Host helpers used by syscalls =====

pub fn kv_get(key_ptr: &[u8]) -> Option<Value> {
    CTX.with(|cell| {
        if let Some(ctx) = cell.borrow().as_ref() {
            ctx.snapshot.get(key_ptr).map(|v| v.to_vec())
        } else { None }
    })
}

pub fn kv_put(key: Key, val: Value) {
    CTX.with(|cell| {
        if let Some(ctx) = cell.borrow_mut().as_mut() {
            ctx.batch.put(key, val);
        }
    });
}

pub fn kv_del(key: Key) {
    CTX.with(|cell| {
        if let Some(ctx) = cell.borrow_mut().as_mut() {
            ctx.batch.del(key);
        }
    });
}

// Scratch helpers: host кладёт данные, программа их затем читает через отдельный syscall
pub fn scratch_set(data: Vec<u8>) {
    CTX.with(|cell| {
        if let Some(ctx) = cell.borrow_mut().as_mut() {
            ctx.scratch = data;
        }
    });
}

pub fn scratch_len() -> usize {
    CTX.with(|cell| {
        if let Some(ctx) = cell.borrow().as_ref() { ctx.scratch.len() } else { 0 }
    })
}

pub fn scratch_read(dst: &mut [u8], offset: usize) -> usize {
    CTX.with(|cell| {
        if let Some(ctx) = cell.borrow().as_ref() {
            let s = &ctx.scratch;
            if offset >= s.len() { return 0; }
            let n = dst.len().min(s.len() - offset);
            dst[..n].copy_from_slice(&s[offset..offset+n]);
            n
        } else { 0 }
    })
}
