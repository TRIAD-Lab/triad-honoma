use crate::tvm::state::KeyAccess;
use std::sync::Arc;
use std::collections::HashMap;

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
        // Индексируем по ключам для ускоренного определения конфликтов
        let n = items.len();
        if n == 0 { return ScheduledBatch { groups: Vec::new() }; }
        let mut items_opt: Vec<Option<BatchItem<T>>> = items.into_iter().map(Some).collect();
        let mut used = vec![false; n];
        let mut out = ScheduledBatch { groups: Vec::new() };

        // Построим индексы: для каждого ключа список читателей и писателей
        let mut writers: HashMap<Vec<u8>, Vec<usize>> = HashMap::new();
        let mut readers: HashMap<Vec<u8>, Vec<usize>> = HashMap::new();
        for (i, it) in items_opt.iter().enumerate() {
            let acc = &it.as_ref().unwrap().access;
            for k in acc.write_keys.iter() {
                writers.entry(k.clone()).or_default().push(i);
            }
            for k in acc.read_keys.iter() {
                readers.entry(k.clone()).or_default().push(i);
            }
        }

        // Оценим степень конфликта для эвристики упорядочивания
        let mut degree: Vec<usize> = vec![0; n];
        for (i, it) in items_opt.iter().enumerate() {
            let acc = &it.as_ref().unwrap().access;
            let mut d = 0usize;
            for k in acc.write_keys.iter() {
                if let Some(ws) = writers.get(k) { d += ws.len().saturating_sub(1); }
                if let Some(rs) = readers.get(k) { d += rs.len(); }
            }
            for k in acc.read_keys.iter() {
                if let Some(ws) = writers.get(k) { d += ws.len(); }
            }
            degree[i] = d;
        }

        // Жадная упаковка независимых групп
        while used.iter().any(|&u| !u) {
            let mut group = Vec::new();
            let mut forbidden = vec![false; n];
            let mut order: Vec<usize> = (0..n).filter(|&i| !used[i]).collect();
            order.sort_by_key(|&i| degree[i]);

            for i in order {
                if used[i] || forbidden[i] { continue; }
                // Снимем ключи заранее (избегаем borrow после take)
                let (w_keys, r_keys) = {
                    let acc = &items_opt[i].as_ref().unwrap().access;
                    let w: Vec<Vec<u8>> = acc.write_keys.iter().cloned().collect();
                    let r: Vec<Vec<u8>> = acc.read_keys.iter().cloned().collect();
                    (w, r)
                };
                used[i] = true;
                // Отметим конфликтующих как запрещённых
                for k in w_keys.iter() {
                    if let Some(ws) = writers.get(k) { for &j in ws { forbidden[j] = true; } }
                    if let Some(rs) = readers.get(k) { for &j in rs { forbidden[j] = true; } }
                }
                for k in r_keys.iter() {
                    if let Some(ws) = writers.get(k) { for &j in ws { forbidden[j] = true; } }
                }
                if let Some(it) = items_opt[i].take() { group.push(it); }
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

#[allow(dead_code)]
fn next_unused(used: &[bool]) -> Option<(usize, usize)> {
    for (i, u) in used.iter().enumerate() { if !u { return Some((i, 0)); } }
    None
}
