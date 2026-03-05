# Borrow Quirk Test — Plan & Expected Trace

Цель: зафиксировать каноническое поведение SVM в сценарии CPI aliasing + дублирующий аккаунт (readonly→writable) и сверить TRVM(trace) == SVM(trace).

## Структура сценария
- Program A (caller)
- Program B (callee)
- Аккаунт X передан дважды в инструкцию Program A:
  - X (index 0): writable
  - X (index 1): readonly (дубликат того же pubkey)
- CPI из Program A → Program B:
  - Первый вызов: `invoke` с обоими X (writable + readonly)
  - Второй вызов: `invoke_signed` (добавить PDA signer) с теми же X
  - Опционально добавить sysvar (clock) в списке аккаунтов CPI, чтобы проверить aliasing с sysvar
- Порядок аккаунтов фиксирован (как выше).

## Что фиксируем как референс на SVM
- `status`
- `error_code_raw` (u32)
- `logs_hash` (blake3 конкатенации логов, как в trace_spec)
- `accounts_touched_hash` (blake3(sorted(pubkey || rw_flag)))
- `cu_used`
- `exec_index` (порядок исполнения CPI шагов)
- На каком CPI (1-й или 2-й) случился abort, если случился

## Ожидаемые исходы
- Возможные сценарии (нужно зафиксировать фактический из SVM):
  1) Borrow conflict ловится на первом CPI и выдаёт Err с конкретным error_code_raw.
  2) Первый CPI проходит, второй падает из-за aliasing/readonly→writable.
  3) Оба проходят — тогда status OK, но hashes фиксируются для последующих регрессий.

## Формат трассы (см. trace_artifact_spec)
Записать одну JSONL-запись на tx (execution order внутри slot):
- trace_version, tx_sig, slot, exec_index, status, error_code_raw, cu_used, cu_limit,
  logs_hash, accounts_touched_hash, state_root_accounts, state_root_balances,
  return_data_hash, program_ids.

## Шаги
1) Реализовать минимальные программы A и B (B делает no-op, только лог/проверку сигнатуры, чтобы сработали borrow rules).
2) Сконфигурировать тестовую транзакцию с двойным входом X (writable + readonly) и CPI (invoke + invoke_signed), опционально sysvar.
3) Запустить на референсном SVM, сохранить трассу (JSONL) с хешами по spec.
4) Запустить на TRVM, сохранить трассу, сравнить поля: status, error_code_raw, logs_hash, accounts_touched_hash, cu_used, exec_index.
5) Если расхождение — задокументировать как SVM-quirk и завести issue.

## Примечание
Пока нет автомата для генерации/сравнения, можно сохранить референсный JSONL в `artifacts/traces/reference/borrow_quirk_slotX.jsonl` и сравнить руками/скриптом.
