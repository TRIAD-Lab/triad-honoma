# TRIAD SDK Plan (Single File)

> Цель: TRIAD SDK ощущается как Solana SDK, но даёт TRIAD-native возможности. Разработчик пишет почти тот же код, использует те же паттерны, меняет лишь RPC/cluster; опционально включает TRIAD-фичи.

## Легенда статуса
- [ ] Не начато
- [~] В процессе
- [x] Готово

## 1. Архитектура SDK (3 слоя)
- [ ] solana-compat — знакомый мир, сохранение API/ощущений.
- [ ] triad-core — новые примитивы (опциональный opt-in).
- [ ] extensions — TRIAD-only «вау»-режим.
- [ ] По умолчанию Solana-разработчик остаётся в solana-compat.

## 2. solana-compat слой (НЕ ТРОГАТЬ ОЩУЩЕНИЯ)
### 2.1 API/импорты
- [ ] Копия Solana API: pubkey::Pubkey, account::AccountInfo, program::{invoke, invoke_signed}, sysvar::{clock::Clock, rent::Rent} и т.п.
- [ ] Никаких терминов wave/interference/dag в этом слое.
### 2.2 Tooling
- [ ] Anchor compatibility или fork (anchor-lang → triad-anchor).
- [ ] BPF build toolchain, IDL generation, macros (#[program], #[account]).
- [ ] Поддержка существующих солана-инструментов.
### 2.3 Поведение
- [ ] Ошибки, CU-лимиты, syscalls, CPI правила — идентичны Solana.

## 3. triad-core слой (новая логика, opt-in)
### 3.1 Execution Context
- [ ] ctx.execution_mode() (deterministic/speculative).
- [ ] ctx.parallelism_hint() (auto/max/bounded).
- [ ] ctx.wave_budget() вместо CU.
- [ ] default = solana behavior, opt-in — осознанный.
- [ ] wave_budget — внутренний контроль; наружу всегда репортится CU (совместимость и спокойствие аудиторов).
### 3.2 State primitives (DAG-aware)
- [ ] #[triad::state] с versioned state, conflict detection, merge semantics (наружу — обычный Rust struct).
### 3.3 Deterministic parallel sections
- [ ] triad::parallel! проверяет side-effects, компилирует в wave-агрегируемую форму; fallback → sequential при невозможности.

## 4. Extensions (TRIAD-only)
### 4.1 ZK hooks
- [ ] #[triad::zk] auto proof generation, proof attached to tx, verifier = chain.
### 4.2 Native batching
- [ ] triad::batch! объединяет вызовы в одну execution wave, снижает overhead (в Solana это были бы N tx).
### 4.3 Cross-program fusion
- [ ] triad::fuse!(...) компилирует в единый execution graph, минимизируя CPI overhead.

## 5. RPC & Client SDK
### 5.1 Solana-compatible RPC (must-have)
- [ ] getProgramAccounts, simulateTransaction, getLogs, getRecentBlockhash.
### 5.2 TRIAD-native RPC (расширение)
- [ ] getExecutionGraph(tx), getWaveStats(block), simulateParallel(tx, mode="speculative").
- [ ] Solana-разработчик может не знать об этих методах.

## 6. Error model
- [ ] Сохранить Solana error codes/order/panic semantics.
- [ ] Добавить TriadError::WaveConflict, TriadError::SpeculationRollback только при opt-in.

## 7. Tooling вокруг SDK
### 7.1 CLI
- [ ] triad new program | triad test | triad deploy --cluster testnet | triad bench.
- [ ] UX совместим с solana-cli; расширенные команды не мешают.
### 7.2 Local validator
- [ ] solana-test-validator experience; deterministic replay; wave-visualization.

## 8. Migration path
### 8.1 5-минутный путь
- [ ] solana program build → triad program deploy без переписывания кода.
### 8.2 Progressive enhancement
- [ ] Просто порт → включить triad metrics → parallel sections → zk/batching (постепенно).

## 9. Acceptance criteria SDK
- [ ] 95% популярных Solana программ компилируются без изменений.
- [ ] Anchor examples работают.
- [ ] Same bytecode → same behavior.
- [ ] Performance gain measurable.
- [ ] Dev понимает SDK без 200-страничных доков.

## 10. Developer perception (итог)
- [ ] «Это Solana… но быстрее и удобнее» — целевой UX.
