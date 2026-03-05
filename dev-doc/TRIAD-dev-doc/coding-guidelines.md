# TRIAD Coding Guidelines — Performance-First Principles

## Mission
Строим **самый быстрый L1 в мире**. Каждая строка кода оптимизирована для максимальной производительности.

---

## Принципы (не абсолютные запреты)

> **Performance & Versatility First**: Оптимизация важна, но гибкость и адаптивность делают сеть жизнеспособной.

### 🟡 МИНИМИЗИРОВАТЬ: `mut` (используйте обоснованно)

**Предпочитайте immutable:**
```rust
// ✅ GOOD — functional style для чистой логики
let counter = operations.iter().filter(|op| op.is_valid()).count();
```

**Допустимо `mut` когда:**
- Построение блоков/батчей в consensus
- Буферы для сетевого I/O
- Локальное состояние ноды (metrics, queues, caches)
- Hot loops с доказанной производительностью
- State machines с четкими переходами

```rust
// ✅ JUSTIFIED — block building requires mutable state
pub struct BlockBuilder {
    transactions: Vec<Transaction>,  // mut through methods
    state_root: Hash,
}

impl BlockBuilder {
    pub fn add_transaction(&mut self, tx: Transaction) {
        self.transactions.push(tx);
        self.state_root = self.compute_state_root();
    }
}
```

**Маркируйте обоснование:**
```rust
let mut buffer = Vec::with_capacity(4096); // justified mut: reusable I/O buffer
```

---

### 🟡 МИНИМИЗИРОВАТЬ: `.clone()` (но иногда оправдано)

**Избегайте в hot paths:**
```rust
// ❌ BAD — clone в горячем пути
for wave in waves {
    let copy = wave.clone();
    process(copy);
}

// ✅ GOOD — zero-copy
for wave in &waves {
    process(wave);
}
```

**Допустимо когда:**
- Clone дешевле блокировок/аллокаций
- Упрощает lifetime tracking без потери производительности  
- Arc::clone для shared ownership (cheap pointer copy)

```rust
// ✅ JUSTIFIED — clone cheaper than lock contention
let snapshot = {
    let state = state_lock.read();
    state.metrics.clone()  // justified clone: cheaper than holding read lock
};
process_metrics(snapshot);

// ✅ JUSTIFIED — Arc clone is just pointer increment
let shared_config = Arc::clone(&config);  // justified clone: Arc reference count
```

**Всегда маркируйте:**
```rust
// justified clone: cheaper than sync cost
// justified clone: Arc reference count
// justified clone: avoids complex lifetime tracking
```

---

### ❌ ИЗБЕГАТЬ: `.unwrap()` / `.expect()` (кроме тестов)

```rust
// ❌ BAD — panic в production
let value = map.get(&key).unwrap();

// ✅ GOOD — explicit error handling
let value = map.get(&key)?;
let value = map.get(&key).ok_or(Error::KeyNotFound)?;

// ✅ OK в тестах
#[test]
fn test_something() {
    let value = result.unwrap();  // Ok in tests
}
```

---

### ❌ ИЗБЕГАТЬ: O(N²) и хуже

```rust
// ❌ BAD — quadratic complexity
for i in 0..n {
    for j in 0..n {
        compute(i, j);
    }
}

// ✅ GOOD — O(N log N) via spatial indexing
let tree = QuadTree::from_points(&points);
for point in points {
    tree.query_neighbors(point, radius);
}

// ✅ GOOD — O(N) via preprocessing
let lookup: AHashMap<_, _> = data.iter()
    .map(|item| (item.id, item))
    .collect();
```

---

## Обязательные паттерны

### ✅ Zero-Copy всегда где возможно
```rust
// Use &[u8] instead of Vec<u8>
fn process_batch(data: &[u8]) -> Result<()>

// Use &str instead of String
fn validate_address(addr: &str) -> bool

// Use slices for iteration
fn sum_amplitudes(waves: &[QuantumWave]) -> f64
```

### ✅ Arena Allocation для массовых аллокаций
```rust
use bumpalo::Bump;

let arena = Bump::new();
let waves: Vec<&QuantumWave> = (0..1000000)
    .map(|i| arena.alloc(create_wave(i)))
    .collect();
// Все освобождается одним drop(arena)
```

### ✅ Object Pooling для горячих путей
```rust
use crossbeam::queue::ArrayQueue;

static BUFFER_POOL: Lazy<ArrayQueue<Vec<u8>>> = Lazy::new(|| {
    let queue = ArrayQueue::new(1024);
    for _ in 0..1024 {
        queue.push(Vec::with_capacity(4096)).ok();
    }
    queue
});

fn process() -> Result<()> {
    let mut buffer = BUFFER_POOL.pop().unwrap_or_else(|| Vec::with_capacity(4096));
    // use buffer
    buffer.clear();
    BUFFER_POOL.push(buffer).ok();
    Ok(())
}
```

### ✅ Lock-Free вместо Mutex
```rust
// ❌ BAD
use std::sync::Mutex;
let counter = Mutex::new(0);

// ✅ GOOD
use std::sync::atomic::{AtomicU64, Ordering};
let counter = AtomicU64::new(0);
counter.fetch_add(1, Ordering::Relaxed);
```

### ✅ SIMD для числовых операций
```rust
use std::simd::*;

fn add_arrays_simd(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .flat_map(|(a_chunk, b_chunk)| {
            let a_vec = f64x4::from_slice(a_chunk);
            let b_vec = f64x4::from_slice(b_chunk);
            (a_vec + b_vec).to_array()
        })
        .collect()
}
```

### ⚖️ БАЛАНС: Compile-time vs Runtime

**Compile-time для:**
- Размеры пулов, шардов, масок
- Криптографические константы
- Структурные параметры (memory layout)

```rust
// ✅ GOOD — compile-time sizing
const SHARD_COUNT: usize = 16;
const SHARD_MASK: u64 = (SHARD_COUNT - 1) as u64;
const MAX_BATCH_SIZE: usize = 10_000;
```

**Runtime configuration для:**
- Параметры газа и лимиты
- ML-адаптивные пороги
- Сетевые таймауты
- Governance parameters
- Тюнинг под аудиторию/рынок

```rust
// ✅ GOOD — runtime flexibility для адаптивности
pub struct ConsensusParams {
    pub gas_limit: u64,           // Adjustable by governance
    pub batch_size: usize,        // Adaptive to network load
    pub sampling_threshold: f32,  // ML-tuned
}

impl ConsensusParams {
    pub fn load_from_config(path: &Path) -> Result<Self> {
        // Runtime configuration для гибкости сети
    }
}
```

**Принцип**: Структурные константы — compile-time, поведенческие параметры — runtime.



---

## Структуры данных

### Используй правильные коллекции
```rust
// HashMap → AHashMap (faster)
use ahash::AHashMap;

// Vec → SmallVec (stack allocation for small sizes)
use smallvec::SmallVec;

// BTreeMap для ordered + range queries
use std::collections::BTreeMap;

// ArrayQueue для lock-free MPMC
use crossbeam::queue::ArrayQueue;
```

### Используй Cow для conditional ownership
```rust
use std::borrow::Cow;

fn process<'a>(data: Cow<'a, [u8]>) -> Result<Cow<'a, [u8]>> {
    if needs_modification(&data) {
        let mut owned = data.into_owned();
        modify(&mut owned);
        Ok(Cow::Owned(owned))
    } else {
        Ok(data) // Zero copy!
    }
}
```

---

## Профилирование обязательно

### Перед оптимизацией
```bash
cargo flamegraph --bin triad-node
cargo bench
```

### Измеряй всё
```rust
#[cfg(feature = "profiling")]
let _timer = metrics::timer!("function_name");

// В production
let start = Instant::now();
let result = expensive_operation();
histogram!("operation_duration_ms").record(start.elapsed().as_millis() as f64);
```

---

## Память

### Используй `#[repr(C)]` для layout control
```rust
#[repr(C)]
struct QuantumWave {
    amplitude: f64,  // 8 bytes
    phase: f64,      // 8 bytes
    position: [f32; 3], // 12 bytes
    // Total: 28 bytes (vs 32+ with default layout)
}
```

### Используй `SmallVec` для малых коллекций
```rust
use smallvec::{SmallVec, smallvec};

// Stack allocation для ≤8 элементов
type ShardList = SmallVec<[u8; 8]>;
```

### Избегай индирекции
```rust
// ❌ BAD — 2 indirections
let data: Arc<Box<Vec<u8>>>;

// ✅ GOOD — 1 indirection
let data: Arc<Vec<u8>>;

// ✅ BETTER — 0 indirections (if possible)
let data: Vec<u8>;
```

---

## Конкурентность

### Rayon для параллелизма
```rust
use rayon::prelude::*;

let results: Vec<_> = data.par_iter()
    .map(|item| expensive_compute(item))
    .collect();
```

### Crossbeam для lock-free
```rust
use crossbeam::channel::unbounded;

let (tx, rx) = unbounded();
// MPMC, lock-free
```

---

## Критерии приёма кода

Перед merge каждый PR должен:
- [ ] **Производительность**: hot paths профилированы
- [ ] **Clones обоснованы**: каждый `.clone()` имеет комментарий `// justified clone: ...`
- [ ] **No unwrap**: отсутствуют `.unwrap()` / `.expect()` (кроме тестов)
- [ ] **Алгоритмы**: O(N log N) или лучше для критических операций
- [ ] **Benchmarks**: есть bench tests для hot paths (или существующие не регрессировали)
- [ ] **Zero-copy**: используется где это практично
- [ ] **Mut обоснован**: мутабельность где необходима, immutable где возможно

### Обоснования принимаются если:
- Clone дешевле альтернативы (locks, complex lifetimes)
- Mut нужен для state management (blocks, buffers, caches)
- Runtime config для адаптивности сети
- Бенчмарки подтверждают решение



---

## Примеры из TRIAD

### MinkowskiWave (правильно)
```rust
#[repr(C)]
pub struct MinkowskiWave {
    pub amplitude: f32,       // f32 вместо f64 (50% памяти)
    pub phase: f32,
    pub position: [f32; 3],   // 3D position
    pub timestamp: u32,       // Slot number, not full timestamp
}

// Immutable by default
impl MinkowskiWave {
    pub fn energy(&self) -> f32 {
        self.amplitude * self.amplitude
    }
    
    // Returns new wave, original unchanged
    pub fn with_decay(self, factor: f32) -> Self {
        Self {
            amplitude: self.amplitude * factor,
            ..self
        }
    }
}
```

### Batch processing (zero-copy)
```rust
pub fn process_batch(waves: &[MinkowskiWave]) -> Result<InterferencePattern> {
    // No clone, no allocation, pure computation
    let energy: f32 = waves.iter()
        .map(|w| w.energy())
        .sum();
    
    Ok(InterferencePattern { energy, .. })
}
```

---

## Философия

> "The fastest code is the code that doesn't run."
> "The fastest memory is the memory you don't allocate."
> "The fastest lock is no lock."

Каждая аллокация — это враг.
Каждый clone — это потеря производительности.
Каждый unwrap — это потенциальный panic в production.

Мы строим систему для **1M+ TPS**. Нет места для небрежности.
