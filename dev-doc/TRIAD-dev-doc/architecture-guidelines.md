# Architecture Guidelines - Организация файлов и модулей

## Принцип организации

> **Золотое правило**: Каждая функциональность должна быть в своей подпапке, никаких разбросанных файлов в корне модуля.

### ❌ ПЛОХО - Беспорядок
```
network/
├── handler.rs
├── message.rs
├── rpc_server.rs
├── rpc_client.rs
├── websocket.rs
├── discovery.rs
├── quantum_handler.rs
└── mod.rs
```

### ✅ ХОРОШО - Организованная структура
```
network/
├── rpc/
│   ├── server.rs
│   ├── client.rs
│   └── mod.rs
├── websocket/
│   ├── server.rs
│   ├── handler.rs
│   └── mod.rs
├── discovery/
│   ├── protocol.rs
│   ├── quic_transport.rs
│   └── mod.rs
├── handler.rs       # Только core handler
└── mod.rs
```

---

## Модули верхнего уровня (src/)

### 1. `quantum/` - Квантовый движок

```
quantum/
├── field/           # Quantum field implementation
│   ├── sharded.rs   # Sharded field with lock-free storage
│   ├── storage.rs   # Wave storage backends
│   └── mod.rs
├── interference/    # Interference calculations
│   ├── engine.rs    # Main interference engine
│   ├── simd.rs      # SIMD optimizations
│   ├── barnes_hut.rs # Spatial optimization
│   └── mod.rs
├── consensus/       # Consensus logic
│   ├── node.rs      # ConsensusNode
│   ├── message.rs   # ConsensusMessage
│   └── mod.rs
├── tests/           # Module tests
│   ├── field_test.rs
│   ├── interference_test.rs
│   └── mod.rs
└── mod.rs
```

**Правила**:
- `field/` - всё что связано с QuantumField
- `interference/` - расчеты интерференции
- `consensus/` - логика консенсуса
- Никаких файлов типа `field.rs`, `interference.rs` в корне!

---

### 2. `network/` - Сетевой слой

```
network/
├── rpc/
│   ├── server.rs
│   ├── client.rs
│   └── mod.rs
├── websocket/
│   ├── server.rs
│   ├── handler.rs
│   └── mod.rs
├── discovery/
│   ├── protocol.rs
│   ├── quic_transport.rs
│   └── mod.rs
├── p2p/
│   ├── peer.rs
│   ├── gossip.rs
│   └── mod.rs
├── transport.rs     # Core transport abstraction
├── error.rs
└── mod.rs
```

**Правила**:
- Каждый протокол (RPC, WebSocket, P2P) в своей папке
- `discovery/` для peer discovery
- `transport.rs` - только core abstraction

---

### 3. `crypto/` - Криптография

```
crypto/
├── schnorr/
│   ├── keypair.rs
│   ├── signature.rs
│   ├── batch.rs
│   └── mod.rs
├── hash/
│   ├── blake3.rs
│   └── mod.rs
├── merkle/
│   ├── tree.rs
│   ├── proof.rs
│   └── mod.rs
└── mod.rs
```

**Правила**:
- Каждая crypto-схема в своей папке
- `schnorr/` - полная изоляция Schnorr логики
- Никаких `schnorr.rs`, `hash.rs` в корне!

---

### 4. `pipeline/` - Validation Pipeline

```
pipeline/
├── hard_checks/
│   ├── orchestrator.rs  # HardChecksPipeline
│   ├── schnorr_stage.rs
│   ├── balance_stage.rs
│   ├── minimal_checks.rs
│   └── mod.rs
├── config.rs
├── metrics.rs
├── stages.rs
└── mod.rs
```

**Правила**:
- `hard_checks/` - всё что относится к validation pipeline
- Каждый stage изолирован
- Metrics и config в корне модуля (shared)

---

### 5. `transaction/` - Обработка транзакций

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
├── validation/
│   ├── signature.rs
│   ├── balance.rs
│   └── mod.rs
└── mod.rs
```

---

### 6. `tvm/` - TRIAD Virtual Machine

```
tvm/
├── vm/
│   ├── executor.rs
│   ├── memory.rs
│   └── mod.rs
├── syscalls/
│   ├── blake3.rs
│   ├── ristretto.rs
│   └── mod.rs
├── state/
│   ├── world_state.rs
│   └── mod.rs
├── scheduler.rs
└── mod.rs
```

---

### 7. `sharding/` - Sharding Logic

```
sharding/
├── coordinator/
│   ├── manager.rs
│   └── mod.rs
├── shard/
│   ├── shard.rs
│   ├── routing.rs
│   └── mod.rs
└── mod.rs
```

---

## Правила именования

### Файлы

- `snake_case.rs` для всех файлов
- `mod.rs` для модуля
- Имя файла = основная структура/функция в нём
  - ✅ `server.rs` содержит `WebSocketServer`
  - ❌ `ws.rs` - слишком короткое

### Структуры

- `PascalCase` для типов
- Префиксы по модулю:
  - `QuantumField`, `QuantumWave` - quantum module
  - `ConsensusNode`, `ConsensusMessage` - consensus
  - `SchnorrKeypair`, `SchnorrSignature` - crypto

### Функции

- `snake_case` для функций
- Глаголы для действий: `add_wave`, `verify_signature`, `calculate_interference`
- Существительные для геттеров: `wave_count`, `active_shards`

---

## Принцип модульности

### 1. Один модуль = одна ответственность

```rust
// ✅ GOOD
mod quantum {
    // Только квантовая логика
}

mod network {
    // Только сетевая логика
}

// ❌ BAD - смешивание ответственностей
mod quantum {
    pub fn send_consensus_message() {} // Это network!
}
```

### 2. Изоляция зависимостей

```rust
// quantum/mod.rs
pub mod field;
pub mod interference;
pub mod consensus;

// Экспортируем только публичные API
pub use field::ShardedQuantumField;
pub use interference::InterferenceEngine;
pub use consensus::ConsensusNode;

// Внутренние детали скрыты
```

### 3. Тестирование по модулям

```
module/
├── core_logic.rs
├── tests/
│   ├── core_logic_test.rs
│   ├── integration_test.rs
│   └── mod.rs
└── mod.rs
```

**Правила тестов**:
- Unit tests в `tests/` внутри модуля
- Integration tests в `tests/` в корне проекта
- Benchmarks в `benches/`

---

## Зависимости между модулями

### Граф зависимостей

```
crypto (базовый уровень)
  ↑
transaction
  ↑
quantum ← network
  ↑         ↑
pipeline  ← sharding
  ↑
tvm
```

### Правила:
- Низкоуровневые модули НЕ зависят от высокоуровневых
- `crypto` не знает про `quantum`
- `quantum` не знает про `pipeline`
- Используйте traits для инверсии зависимостей

---

## Миграция существующего кода

Если модуль не следует guidelines:

1. Создать папку `module/legacy/`
2. Переместить старые файлы туда
3. Пометить `#deprecated` в комментариях
4. Постепенно рефакторить в новую структуру

Пример:
```
network/
├── legacy/
│   ├── old_handler.rs  // #deprecated
│   └── mod.rs
├── rpc/               // New structure
│   └── ...
└── mod.rs
```

---

## Проверка структуры

### Автоматическая проверка (pre-commit hook)

```bash
#!/bin/bash
# Проверка что в корне модулей нет лишних файлов

for module in quantum network crypto pipeline transaction tvm sharding; do
    file_count=$(find src/$module -maxdepth 1 -name "*.rs" ! -name "mod.rs" | wc -l)
    if [ $file_count -gt 2 ]; then
        echo "ERROR: Too many files in src/$module root"
        exit 1
    fi
done
```

### Code review чеклист

- [ ] Новые файлы в правильных подпапках
- [ ] Нет похожих файлов разбросанных по разным местам
- [ ] Публичный API экспортирован через mod.rs
- [ ] Тесты в tests/ подпапке
- [ ] Зависимости не нарушают граф

---

## Примеры из проекта

### До (беспорядок)
```
network/
├── handler.rs
├── quantum_handler.rs
├── websocket_server.rs
├── discovery.rs
└── transport.rs
```

### После (организованно)
```
network/
├── handlers/
│   ├── core.rs         # Base handler trait
│   ├── quantum.rs      # QuantumHandler
│   └── mod.rs
├── websocket/
│   ├── server.rs
│   └── mod.rs
├── discovery/
│   ├── protocol.rs
│   └── mod.rs
└── mod.rs
```

---

## Ключевые принципы

- **Минимум файлов в корне** - максимум 3 `.rs` файла в корне модуля (mod.rs + error.rs + core.rs)
- **Подпапки по необходимости** - создавайте столько подпапок, сколько нужно для чёткой организации
- **Одна ответственность** - каждая подпапка должна иметь чёткую, единственную ответственность
- **Баланс глубины** - избегайте как слишком плоской (всё в корне), так и слишком глубокой (> 4 уровней) вложенности

## Инструменты

### Автоматическая реорганизация

```bash
# Скрипт для реорганизации модуля
./scripts/reorganize_module.sh network
```

### Визуализация структуры

```bash
# Показать дерево модулей
tree src/ -d -L 2
```

---

**Помните**: Хорошо организованный код = легкий аудит = меньше багов = быстра разработка.
