use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, RwLock};

pub type Key = Vec<u8>;
pub type Value = Vec<u8>;

#[derive(Debug, Clone, Default)]
pub struct KeyAccess {
    pub read_keys: HashSet<Key>,
    pub write_keys: HashSet<Key>,
}

impl KeyAccess {
    pub fn new() -> Self { Self { read_keys: HashSet::new(), write_keys: HashSet::new() } }
    pub fn read(mut self, k: Key) -> Self { self.read_keys.insert(k); self }
    pub fn write(mut self, k: Key) -> Self { self.write_keys.insert(k); self }
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    version: u64,
    // Иммутабельный срез состояния на момент снимка
    view: Arc<BTreeMap<Key, Value>>, 
}

impl Snapshot {
    pub fn version(&self) -> u64 { self.version }
    pub fn get(&self, key: &[u8]) -> Option<&[u8]> { self.view.get(key).map(|v| v.as_slice()) }
}

#[derive(Debug, Default)]
pub struct WriteBatch {
    pub puts: BTreeMap<Key, Value>,
    pub deletes: HashSet<Key>,
    // Numeric additive updates (for u128 LE values), split into non-overflowing components
    pub incrs: BTreeMap<Key, u128>,
    pub decrs: BTreeMap<Key, u128>,
}

impl WriteBatch {
    pub fn new() -> Self { Self::default() }
    pub fn put(&mut self, k: Key, v: Value) { self.puts.insert(k, v); }
    pub fn del(&mut self, k: Key) { self.deletes.insert(k); }
    pub fn add_u128_le_increase(&mut self, k: Key, amount: u128) {
        use std::collections::btree_map::Entry;
        match self.incrs.entry(k) {
            Entry::Occupied(mut e) => { *e.get_mut() = e.get().saturating_add(amount); },
            Entry::Vacant(e) => { e.insert(amount); },
        }
    }
    pub fn add_u128_le_decrease(&mut self, k: Key, amount: u128) {
        use std::collections::btree_map::Entry;
        match self.decrs.entry(k) {
            Entry::Occupied(mut e) => { *e.get_mut() = e.get().saturating_add(amount); },
            Entry::Vacant(e) => { e.insert(amount); },
        }
    }
    pub fn is_empty(&self) -> bool { self.puts.is_empty() && self.deletes.is_empty() && self.incrs.is_empty() && self.decrs.is_empty() }
}

#[derive(Debug, Default)]
pub struct StateInner {
    // Текущее коммитнутое состояние
    kv: BTreeMap<Key, Value>,
    // Текущая версия
    ver: u64,
}

#[derive(Debug, Clone, Default)]
pub struct StateView(Arc<RwLock<StateInner>>);

impl StateView {
    pub fn new() -> Self { Self::default() }

    // MVCC snapshot (copy-on-read структуры через Arc)
    pub fn snapshot(&self) -> Snapshot {
        let inner = self.0.read().unwrap();
        Snapshot { version: inner.ver, view: Arc::new(inner.kv.clone()) }
    }

    pub fn get(&self, key: &[u8]) -> Option<Value> {
        let inner = self.0.read().unwrap();
        inner.kv.get(key).cloned()
    }

    pub fn put_batch(&self, wb: &WriteBatch) {
        if wb.is_empty() { return; }
        let mut inner = self.0.write().unwrap();
        // Apply additive deltas first to avoid lost updates across independent batches
        if !wb.incrs.is_empty() || !wb.decrs.is_empty() {
            use std::collections::BTreeSet;
            let mut keys: BTreeSet<&Key> = BTreeSet::new();
            for k in wb.incrs.keys() { keys.insert(k); }
            for k in wb.decrs.keys() { keys.insert(k); }
            for k in keys {
                let mut curr: u128 = 0;
                if let Some(v) = inner.kv.get(k) {
                    if v.len() >= 16 {
                        let mut arr = [0u8; 16];
                        arr.copy_from_slice(&v[..16]);
                        curr = u128::from_le_bytes(arr);
                    }
                }
                if let Some(add) = wb.incrs.get(k) { curr = curr.saturating_add(*add); }
                if let Some(sub) = wb.decrs.get(k) { curr = curr.saturating_sub(*sub); }
                inner.kv.insert(k.clone().clone(), curr.to_le_bytes().to_vec());
            }
        }
        for (k, v) in wb.puts.iter() { inner.kv.insert(k.clone(), v.clone()); }
        for k in wb.deletes.iter() { inner.kv.remove(k); }
        inner.ver = inner.ver.saturating_add(1);
    }
}

// Утилита: формируем read/write sets по контракту
pub fn union_key_access(accs: &[KeyAccess]) -> KeyAccess {
    let mut out = KeyAccess::new();
    for a in accs {
        out.read_keys.extend(a.read_keys.iter().cloned());
        out.write_keys.extend(a.write_keys.iter().cloned());
    }
    out
}
