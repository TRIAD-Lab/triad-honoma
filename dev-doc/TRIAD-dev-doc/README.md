# TRIAD Development Documentation

Документация для разработки L1 blockchain TRIAD с квантово-интерференционным консенсусом.

## Структура документации

### Основные документы

- **[module-plan.md](./module-plan.md)** - План разработки по модулям (16 фаз)
- **[coding-guidelines.md](./coding-guidelines.md)** - Стандарты написания кода
- **[architecture-guidelines.md](./architecture-guidelines.md)** - Организация файлов и модулей
- **[api-reference.md](./api-reference.md)** - Справочник API модулей

### Прогресс

- **[completed-work.md](./completed-work.md)** - Что уже сделано
- **[current-status.md](./current-status.md)** - Текущее состояние проекта

### Технические спецификации

- **[minkowski-consensus.md](./minkowski-consensus.md)** - Спецификация консенсуса
- **[performance-targets.md](./performance-targets.md)** - Целевые метрики производительности

## Quick Start для разработчика

1. Прочитать [coding-guidelines.md](./coding-guidelines.md)
2. Изучить [architecture-guidelines.md](./architecture-guidelines.md)
3. Посмотреть [module-plan.md](./module-plan.md) для выбора задачи
4. Следовать принципам: **Zero-copy**, **Lock-free**, **No clones**

## Принципы проекта

- **Performance-first** - каждая строка оптимизирована
- **Modular organization** - четкая структура папок
- **Deterministic** - одинаковый input → одинаковый output
- **Scalable** - линейная масштабируемость

## Контакты и поддержка

- GitHub Issues для багов
- Discussions для вопросов
- Code review обязателен для всех PR
