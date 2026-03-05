# Module Development Plan - 16 Phases Mapped to Modules

> Организация по **модулям верхнего уровня**, а не по подмодулям

## Модули проекта

### 1. `crypto/` - ✅ Криптография (DONE)
### 2. `quantum/` - 🟡 Квантовый движок (IN PROGRESS)  
### 3. `pipeline/` - 🟡 Валидация (PARTIAL)
### 4. `transaction/` - 🟡 Обработка транзакций (NEEDS WORK)
### 5. `network/` - 🔴 Сетевой слой (NEEDS REORGANIZATION)
### 6. `tvm/` - 🟢 Виртуальная машина (STABLE)
### 7. `sharding/` - 🟢 Шардинг (STABLE)
### 8. `error_analysis/` - 🟢 Анализ ошибок (STABLE)
### 9. `semantic/` - 🟢 Семантический анализ (STABLE)
### 10. `memory/` - 🟢 Управление памятью (STABLE)

---

## MODULE 1: `crypto/` - Криптография ✅

**Статус**: Завершен (Phase 2)

### Сделано
- [x] Schnорр подписи на Ristretto255
- [x] Blake3 хеширование
- [x] Batch verification
- [x] Schnorr keypair generation

### Следующие улучшения (Phase 6)
- [ ] FROST threshold signatures
- [ ] MuSig2 aggregation
- [ ] Merkle tree proofs

**Файлы**:
- `schnorr.rs` - основная криптография (должна быть в `schnorr/mod.rs`)
- `bls/` - legacy, удалить
- `ristretto/` - legacy, удалить

**Приоритет**: Низкий (модуль готов)

---

## MODULE 2: `quantum/` - Квантовый движок 🟢
 
 **Статус**: Базовая оптимизация завершена, нужны алгоритмические улучшения
 
 ### Phase 1: Core Wave Engine
 - [x] MinkowskiWave struct (f32 fields)
 - [x] Amplitude/phase calculation  
 - [x] 3-torus position mapping
 - [x] Causal filter (light cone)
 - [x] Interference calculation
 
 ### Phase 4: Optimizations  
 - [x] Barnes-Hut algorithm (QuadTree/Octree)

 - [ ] Causal bucketing
 - [x] SIMD для sum_cos_pairs
 - [ ] Sampling & Monte-Carlo
 
 ### Phase 5: Pruning
 - [ ] Marginal contribution detection
 - [ ] Iterative pruning loop
 - [ ] Fallback UTXO check
 
 ### Критические задачи
 1. **Убрать все `.clone()` из hot paths** (Partially done)
 2. **Lock-free storage** (✅ Done: DashMap + Atomic)
 3. **Arena allocation** (✅ Done: Buffer Pooling)
 4. **SIMD** (✅ Done: AVX2)
 5. **QuadTree** для causal filter (Next Step)
 
 **Файлы** (Реорганизация выполнена ✅):
 ```
 quantum/
 ├── field/
 │   ├── sharded.rs
 │   ├── storage.rs
 │   └── mod.rs
 ├── interference/
 │   ├── engine.rs
 │   ├── simd.rs
 │   └── mod.rs
 ├── consensus/
 │   └── ...
 └── mod.rs
 ```
 
 **Приоритет**: ⚠️ ВЫСОКИЙ (performance bottleneck)

---

## MODULE 3: `pipeline/` - Transaction Validation 🟡

**Статус**: Base infrastructure ready, stages need implementation

### Phase 3: Hard Checks Pipeline
- [x] HardChecksPipeline structure
- [x] Schnorr batch verification stage
- [ ] Balance/UTXO stage **← NEXT**
- [ ] Minimal DAG checks (parent existence only)
- [ ] Rate limiting stage
- [ ] SLA validation tooling (< 200 ms @ 100 000 transactions)

### Критические задачи
1. **Упростить DAG validation** (не полный topological sort!)
2. **Lock-free balance cache** (DashMap)
3. **Batch size optimization**  (адаптивные батчи)
4. **Latency target**: < 200 ms для 100 000 transactions

**Файлы** (нужна реорганизация):
```
pipeline/
├── hard_checks/
│   ├── orchestrator.rs
│   ├── schnorr_stage.rs
│   ├── balance_stage.rs    ← CREATE
│   ├── minimal_checks.rs   ← CREATE
│   └── mod.rs
├── config.rs
├── metrics.rs
└── mod.rs
```

**Текущее состояние**:
- `hard_checks.rs` ✅
- `schnorr_stage.rs` ✅  
- `dag_stage.rs` - создан, но не используется ❌

**Приоритет**: ⚠️ ВЫСОКИЙ (критический путь консенсуса)

---

## MODULE 4: `transaction/` - Обработка транзакций 🟡

**Статус**: Работает, но с mutex

### Phase 1-2: Core Processing
- [x] Transaction struct
- [x] Wave creation from transaction
- [x] Schnorr подписи для transactions

### Критические задачи
1. **Убрать `QUANTUM_FIELD` global mutex**
2. **Lock-free transaction cache** (DashMap)
3. **Zero-copy wave creation**
4. **Reuse buffers** для serialization

**Файлы** (нужна реорганизация):
```
transaction/
├── processor/
│   ├── core.rs
│   ├── wave_creation.rs
│   └── mod.rs
├── types/
│   ├── transaction.rs
│   ├── smart_contract.rs
│   └── mod.rs
└── mod.rs
```

**Текущее состояние**:
- `processor.rs` - монолит с глобальным mutex ❌
- `mod.rs` - типы разбросаны ❌

**Приоритет**: 🔶 СРЕДНИЙ

---

## MODULE 5: `network/` - Сетевой слой 🔴

**Статус**: Беспорядок, нужна полная реорганизация

### Phase 8: Interoperability
- [ ] Blockchain format adapters
- [ ] Cross-chain bridge
- [ ] Merkle proofs для bridge

### Phase 9: Telemetry
- [ ] Real-time metrics
- [ ] Prometheus integration
- [ ] Structured logging

### Критические задачи (архитектура)
1. **Реорганизовать файлы** по подпапкам
2. **RPC в отдельную папку** `rpc/`
3. **WebSocket в отдельную папку** `websocket/`
4. **Discovery в отдельную папку** `discovery/`
5. **Интеграционные тесты** для сетевых путей (вклад в ≥ 85% coverage)

**Текущее состояние** (ПЛОХО):
```
network/
├── handler.rs
├── quantum_handler.rs
├── websocket_server.rs
├── discovery/
│   └── quic_transport.rs
├── transport.rs
└── mod.rs
```

**Целевая структура**:
```
network/
├── handlers/
│   ├── core.rs
│   ├── quantum.rs
│   └── mod.rs
├── websocket/
│   ├── server.rs
│   └── mod.rs
├── discovery/
│   ├── protocol.rs
│   ├── quic_transport.rs
│   └── mod.rs
├── rpc/
│   ├── server.rs
│   ├── client.rs
│   └── mod.rs
└── mod.rs
```

**Приоритет**: 🔶 СРЕДНИЙ (работает, но неорганизованно)

---

## MODULE 6: `tvm/` - TRIAD Virtual Machine 🟡

**Статус**: MAJOR REFACTOR (SVM Transformation)

### Phase 1: Integration (Current)
- [x] Research & Audit (SVM Architecture)
- [x] Design Master Plan (`dev-doc/tvm_implementation_master_plan.md`)
- [ ] Implement `InterferenceScheduler` (InstalledScheduler trait)
- [ ] Link `solana-runtime`

### Phase 2: Lock Removal
- [ ] Patch `solana-runtime` to bypass `lock_accounts`
- [ ] Trust Interference Engine for safety

### Phase 3: Zero-Copy Memory
- [ ] Implement `ShardedQuantumField`
- [ ] Rewrite `TransactionContext` for raw pointers

### Текущее состояние
- Переход от кастомной VM к SVM-based архитектуре.
- Цель: 97% параллелизма через Interference Engine.

**Приоритет**: ⚠️ ВЫСОКИЙ (Core Feature)

---

## MODULE 7: `sharding/` - Sharding 🟢

**Статус** Стабилен

### Phase 15: Stress Testing
- [ ] Multi-shard reorg simulation
- [ ] Byzantine resistance
- [ ] Fork convergence tests

### Текущее состояние
- ✅ Shard coordinator
- ✅ Cross-shard messaging
- ✅ Routing

**Приоритет**: 🟢 НИЗКИЙ (stable)

---

## MODULE 8-10: Support Modules 🟢

### `error_analysis/` - Анализ ошибок
- ✅ Error detection
- ✅ ML-ready error classification

### `semantic/` - Семантический анализ
- ✅ Goal tracking  
- ✅ SystemGoal enum

### `memory/` - Управление памятью
- ✅ Custom allocators
- ✅ Memory profiling

**Приоритет**: 🟢 НИЗКИЙ (stable support modules)

---

## График работы (строго модуль за модулем)

### Неделя 1-2: MODULE 2 (`quantum/`)
1. Реорганизация файлов
2. Убрать clones
3. Lock-free storage
4. SIMD optimization

**Цель**: < 50ms interference для 10K waves

### Неделя 3: MODULE 3 (`pipeline/`)
1. Balance stage
2. Minimal checks
3. Rate limiting
4. Latency optimization

**Цель**: < 200ms полный pipeline для 5K tx

### Неделя 4: MODULE 4 (`transaction/`)
1. Убрать global mutex
2. Lock-free cache
3. Zero-copy optimizations

**Цель**: < 5μs wave creation

### Неделя 5: MODULE 5 (`network/`)
1. Файловая реорганизация
2. Разделение по протоколам
3. Clean architecture

**Цель**: Чистая структура для аудита

### Недели 6+: Phases 4-16
- Barnes-Hut
- Pruning
- Testing
- Interop
- Security
- Governance

---

## Performance Targets (по модулям)

| Module | Operation | Target | Current |
|--------|-----------|--------|---------|
| `quantum/` | Interference (10K) | < 50 ms | ~500 ms ❌ |
| `pipeline/` | Full validation (100K) | < 200 ms | ??? |
| `transaction/` | Wave creation | < 5 μs | ??? |
| `crypto/` | Batch verify (1K) | < 150 ms | ✅ Done |
| `tests/` | Integration coverage | ≥ 85% | ??? |

---

## Принципы работы

1. **Один модуль за раз** - не прыгаем между модулями
2. **Реорганизация первая** - сначала файлы, потом оптимизация
3. **Metrics-driven** - измеряем ДО и ПОСЛЕ
4. **Zero clones** - каждый clone должен быть обоснован
5. **Lock-free** - никаких mutex в hot paths

---

## Checklist для каждого модуля

### Before starting
- [ ] Прочитать architecture-guidelines.md
- [ ] Изучить coding-guidelines.md  
- [ ] Создать папки по guidelines

### During work
- [ ] Следовать zero-copy принципам
- [ ] Benchmarks ДО изменений
- [ ] Benchmarks ПОСЛЕ изменений
- [ ] Code review

### Before completion
- [ ] Все `.clone()` обоснованы в комментариях
- [ ] Нет mutex в hot paths
- [ ] Файлы организованы по папкам
- [ ] Tests проходят
- [ ] Documentation updated

---

**Текущий фокус**: MODULE 2 (`quantum/`) - реорганизация и оптимизация
