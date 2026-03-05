# Trace Artifact Specification (TRVM vs SVM)

## Purpose
Детерминированная сверка TRVM и SVM без хранения полных логов: выявлять расхождения по состоянию, CU, порядку исполнения, CPI/borrow-путям.

## Artifact format
- Формат: JSONL, одна запись на транзакцию (или на батч, если пакетная обработка) в порядке **execution order внутри slot**.
- Имя файла: `artifacts/traces/{network}/{slot}.jsonl`.
- Для сравнения TRVM и SVM генерируются идентичные по структуре файлы и сравниваются построчно по execution order.

## Fields per record
- `trace_version`: string, версия формата/хеширования (меняется при смене лог-нормализации/хешей).
- `tx_sig`: base58 сигнатура tx (или synthetic id для внутренних batch-чонков).
- `slot`: u64, номер слота.
- `exec_index`: u32, монотонный индекс исполнения внутри slot (execution order).
- `status`: человекочитаемый итог (OK/Err/... как в SVM API).
- `error_code_raw`: u32, исходный код ошибки (Solana semantics), даже если `status` уже содержит Err.
- `cu_used`: u64, публичный счётчик CU (совпадает с SVM), независим от внутреннего wave-cost.
- `cu_limit`: u64, лимит CU для tx (как в SVM) — помогает ловить abort vs success.
- `logs_hash`: blake3(logs_text), где logs_text — конкатенация логов как выдаёт SVM (строки с \n).
- `accounts_touched_hash`: blake3(sorted(pubkey || rw_flag)), rw_flag ∈ {R,W}; сортировка по байтовому pubkey, затем флаг.
- `state_root_accounts`: хеш (например, merkle/blake3) от содержимого данных аккаунтов после tx.
- `state_root_balances`: хеш от лампортов/балансов после tx.
- `return_data_hash`: blake3(return_data_bytes) или null если нет.
- `program_ids`: список program_id, вовлечённых в исполнение (в порядке встречаемости).

## Ordering
- Записи в файле отсортированы строго по execution order внутри слота (фактический порядок исполнения), **не** по tx_sig. Это позволяет ловить ordering-баги.

## Comparison
- Сравнение в CI: построчно по execution order; поля с hash-значениями сравниваются на равенство.
- При несовпадении логов/статусов/хешей выводится diff по полям; полные логи не требуются.

## Notes
- logs_hash используется вместо хранения логов для детерминизма и компактности.
- error_code_raw критичен: разные пути могут давать одинаковый status, но разный код.
- accounts_touched_hash ловит скрытые расхождения CPI и borrow-путей при одинаковом state_root.
- Разделение state_root на accounts/balances помогает локализовать баги (данные vs лампорты).

## Out of scope
- Полнотекстовые логи и бинарные дампы памяти не храним в этом артефакте; при необходимости включаются отдельно для отладки.
