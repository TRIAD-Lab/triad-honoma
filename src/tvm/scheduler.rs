use crate::tvm::state::KeyAccess;
use std::sync::Arc;

#[derive(Debug)]
pub struct BatchItem<T> {
    pub id: Arc<str>,
    pub payload: T,
    pub access: KeyAccess,
}

#[derive(Debug, Default)]
pub struct ScheduledBatch<T> {
    pub groups: Vec<Vec<BatchItem<T>>>,
}

pub struct Scheduler;

impl Scheduler {
    /// Формирует независимые группы (по отсутствию пересечений write/write и write/read)
    /// Потребляет входной вектор и распределяет элементы по группам без clone().
    pub fn schedule<T>(items: Vec<BatchItem<T>>) -> ScheduledBatch<T> {
        // Жадная раскраска по конфликтам
        let n = items.len();
        let mut items_opt: Vec<Option<BatchItem<T>>> = items.into_iter().map(Some).collect();
        let mut used = vec![false; n];
        let mut out = ScheduledBatch { groups: Vec::new() };

        // Предварительно подсчёт конфликтов
        let mut conflicts: Vec<Vec<usize>> = vec![Vec::new(); n];
        for i in 0..n {
            for j in (i+1)..n {
                if is_conflict(&items_opt[i].as_ref().unwrap().access, &items_opt[j].as_ref().unwrap().access) {
                    conflicts[i].push(j);
                    conflicts[j].push(i);
                }
            }
        }

        // Пока остались неиспользованные элементы — набираем группу максимальной независимости
        while let Some((_start, _)) = next_unused(&used) {
            let mut group = Vec::new();
            let mut forbidden = vec![false; n];
            // выбираем вершины с минимальной степенью конфликта сначала (эвристика)
            let mut order: Vec<usize> = (0..n).filter(|&i| !used[i]).collect();
            order.sort_by_key(|&i| conflicts[i].len());

            for i in order {
                if used[i] || forbidden[i] { continue; }
                // добавляем i в группу
                used[i] = true;
                for &c in &conflicts[i] { forbidden[c] = true; }
                if let Some(it) = items_opt[i].take() {
                    group.push(it);
                }
            }
            if !group.is_empty() { out.groups.push(group); }
        }
        out
    }
}

fn is_conflict(a: &KeyAccess, b: &KeyAccess) -> bool {
    // write/write или write/read пересечение по ключам
    for k in a.write_keys.iter() {
        if b.write_keys.contains(k) || b.read_keys.contains(k) { return true; }
    }
    for k in b.write_keys.iter() {
        if a.read_keys.contains(k) { return true; }
    }
    false
}

fn next_unused(used: &[bool]) -> Option<(usize, usize)> {
    for (i, u) in used.iter().enumerate() { if !u { return Some((i, 0)); } }
    None
}
