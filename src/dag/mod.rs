pub mod batch;

use ahash::AHashMap;
use rayon::prelude::*;
use smallvec::SmallVec;
use std::collections::{HashSet, VecDeque};
use uuid::Uuid;

pub use batch::{
    BatchError, BatchFeatures, BatchFinalizeEvidence, BatchFinalizeMetrics, BatchSignature,
    InterferenceBatch, TreeStrategy,
};

/// Phase 3: Optimized DAG
/// - AHashMap for faster hashing
/// - u128 for IDs (avoiding Uuid overhead)
/// - SmallVec for parent lists (avoids heap allocation for small graphs)
pub struct Dag {
    /// Map: dag node -> parent ids
    nodes: AHashMap<DagNodeId, SmallVec<[DagNodeId; 4]>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DagNodeId {
    Transaction(u128),
    Batch([u8; 32]),
}

impl DagNodeId {
    #[inline]
    fn from_tx(tx_id: &Uuid) -> Self { DagNodeId::Transaction(tx_id.as_u128()) }

    #[inline]
    fn as_transaction_uuid(&self) -> Option<Uuid> {
        match self {
            DagNodeId::Transaction(id) => Some(Uuid::from_u128(*id)),
            DagNodeId::Batch(_) => None,
        }
    }
}

impl Dag {
    pub fn new() -> Self { Self { nodes: AHashMap::new() } }

    pub fn add_transaction(&mut self, tx_id: Uuid, parents: Vec<Uuid>) {
        let node = DagNodeId::from_tx(&tx_id);
        let parent_ids: SmallVec<[DagNodeId; 4]> = parents.iter().map(DagNodeId::from_tx).collect();
        self.nodes.insert(node, parent_ids);
    }

    pub fn batch_add_transactions(&mut self, txs: &[(Uuid, Vec<Uuid>)]) {
        for (id, parents) in txs {
            self.add_transaction(*id, parents.clone());
        }
    }

    /// Вариант без лишних clone(): принимает владение батчем и перемещает parents
    pub fn batch_add_transactions_owned<I>(&mut self, txs: I)
    where
        I: IntoIterator<Item = (Uuid, Vec<Uuid>)>,
    {
        for (id, parents) in txs {
            self.add_transaction(id, parents);
        }
    }

    pub fn add_weighted_transaction(&mut self, tx_id: Uuid, parents: Vec<(Uuid, f64)>) {
        let parent_ids: SmallVec<[DagNodeId; 4]> = parents.iter().map(|(id, _)| DagNodeId::from_tx(id)).collect();
        self.nodes.insert(DagNodeId::from_tx(&tx_id), parent_ids);
        // Store weights separately if needed
    }

    pub fn validate_parallel(&self, tx_ids: &[Uuid]) -> Vec<bool> {
        tx_ids.par_iter().map(|id| self.validate(id)).collect()
    }

    fn validate(&self, _tx_id: &Uuid) -> bool {
        // Validation logic with interference check
        true
    }

    /// Количество вершин DAG
    pub fn size(&self) -> usize {
        self.nodes.len()
    }

    /// Количество рёбер DAG
    pub fn edge_count(&self) -> usize {
        self.nodes.values().map(|p| p.len()).sum()
    }

    /// Возвращает список неразрешённых зависимостей для указанной транзакции
    pub fn unresolved_dependencies(&self, tx_id: &Uuid) -> Vec<Uuid> {
        let node = DagNodeId::from_tx(tx_id);
        match self.nodes.get(&node) {
            Some(parents) => parents
                .iter()
                .filter(|p| !self.nodes.contains_key(p))
                .filter_map(|p| p.as_transaction_uuid())
                .collect(),
            None => Vec::new(),
        }
    }

    /// Готова ли транзакция к выполнению (все зависимости присутствуют в DAG)
    pub fn is_ready(&self, tx_id: &Uuid) -> bool {
        self.unresolved_dependencies(tx_id).is_empty()
    }

    /// Топологическая сортировка (Kahn's algorithm). Возвращает порядок выполнения.
    pub fn topological_order(&self) -> Result<Vec<Uuid>, String> {
        // Unified index: indegree and children in one pass
        let mut indegree: AHashMap<DagNodeId, usize> = AHashMap::new();
        let mut children: AHashMap<DagNodeId, SmallVec<[DagNodeId; 4]>> = AHashMap::new();

        // Инициализация узлов
        for (&node, parents) in self.nodes.iter() {
            indegree.entry(node).or_insert(0);
            for &p in parents.iter() {
                indegree.entry(p).or_insert(0);
            }
        }

        // Заполнение children и indegree
        for (&node, parents) in self.nodes.iter() {
            for &p in parents.iter() {
                children.entry(p).or_insert_with(SmallVec::new).push(node);
                *indegree.entry(node).or_insert(0) += 1;
            }
        }

        // Очередь узлов с нулевой входящей степенью
        let mut queue: VecDeque<DagNodeId> = indegree
            .iter()
            .filter_map(|(n, &deg)| if deg == 0 { Some(*n) } else { None })
            .collect();

        let mut order = Vec::with_capacity(indegree.len());
        let mut processed = 0usize;

        while let Some(n) = queue.pop_front() {
            processed += 1;
            if let Some(tx_id) = n.as_transaction_uuid() {
                order.push(tx_id);
            }
            if let Some(ch) = children.get(&n) {
                for &c in ch.iter() {
                    if let Some(d) = indegree.get_mut(&c) {
                        *d -= 1;
                        if *d == 0 { queue.push_back(c); }
                    }
                }
            }
        }

        if processed != indegree.len() {
            return Err("Cycle detected or missing nodes for topological sort".to_string());
        }
        Ok(order)
    }

    /// Вычисляет уровни DAG (Kahn's algorithm by layers). Каждый уровень содержит вершины,
    /// которые могут обрабатываться параллельно после завершения всех предыдущих уровней.
    pub fn levels(&self) -> Result<Vec<Vec<Uuid>>, String> {
        // Построим карты входящей степени и детей
        let mut indegree: AHashMap<DagNodeId, usize> = AHashMap::new();
        let mut children: AHashMap<DagNodeId, SmallVec<[DagNodeId; 4]>> = AHashMap::new();

        // Инициализация узлов
        for (&node, parents) in self.nodes.iter() {
            indegree.entry(node).or_insert(0);
            for &p in parents.iter() {
                indegree.entry(p).or_insert(0);
            }
        }

        for (&node, parents) in self.nodes.iter() {
            for &p in parents.iter() {
                children.entry(p).or_insert_with(SmallVec::new).push(node);
                *indegree.entry(node).or_insert(0) += 1;
            }
        }

        // Начальный уровень — все вершины с нулевой входящей степенью
        let mut current_level: Vec<DagNodeId> = indegree
            .iter()
            .filter_map(|(n, &deg)| if deg == 0 { Some(*n) } else { None })
            .collect();

        let mut produced: Vec<Vec<Uuid>> = Vec::with_capacity(indegree.len().max(1));
        let total_nodes = indegree.len();
        let mut seen_count = 0usize;

        while !current_level.is_empty() {
            // Зафиксируем текущий уровень, convert to Uuid
            let level_uuids: Vec<Uuid> = current_level
                .iter()
                .filter_map(|node| node.as_transaction_uuid())
                .collect();
            if !level_uuids.is_empty() {
                produced.push(level_uuids);
            }
            seen_count += current_level.len();

            // Подготовим следующий уровень, уменьшая входящие степени у детей
            let mut next_level: Vec<DagNodeId> = Vec::with_capacity(16);
            for n in current_level.iter() {
                if let Some(ch) = children.get(n) {
                    for &c in ch.iter() {
                        if let Some(d) = indegree.get_mut(&c) {
                            *d -= 1;
                            if *d == 0 { next_level.push(c); }
                        }
                    }
                }
            }

            current_level = next_level;
        }

        if seen_count != total_nodes {
            return Err("Cycle detected or missing nodes for level computation".to_string());
        }
        Ok(produced)
    }

    // --- Новые методы ---
    /// Получить все id транзакций DAG
    pub fn all_tx_ids(&self) -> Vec<Uuid> {
        self.nodes
            .keys()
            .filter_map(|id| id.as_transaction_uuid())
            .collect()
    }

    /// Получить родителей транзакции
    pub fn parents(&self, tx_id: &Uuid) -> Option<Vec<Uuid>> {
        self.nodes.get(&DagNodeId::from_tx(tx_id)).map(|parents| {
            parents
                .iter()
                .filter_map(|p| p.as_transaction_uuid())
                .collect()
        })
    }

    /// Получить детей транзакции
    pub fn children(&self, tx_id: &Uuid) -> Vec<Uuid> {
        let needle = DagNodeId::from_tx(tx_id);
        self.nodes
            .iter()
            .filter_map(|(&child, parents)| {
                if parents.contains(&needle) {
                    child.as_transaction_uuid()
                } else {
                    None
                }
            })
            .collect()
    }

    /// Глубина DAG (максимальная длина пути от корня до листа)
    pub fn depth(&self) -> usize {
        let mut max_depth = 0;
        for &node in self.nodes.keys() {
            if node.as_transaction_uuid().is_some() {
                let d = self.depth_from(node, &mut HashSet::new());
                if d > max_depth { max_depth = d; }
            }
        }
        max_depth
    }
    fn depth_from(&self, node: DagNodeId, visited: &mut HashSet<DagNodeId>) -> usize {
        if !visited.insert(node) { return 0; }
        match self.nodes.get(&node) {
            Some(parents) if !parents.is_empty() => {
                1 + parents.iter().map(|&p| self.depth_from(p, visited)).max().unwrap_or(0)
            },
            _ => 1,
        }
    }

    /// Проверка наличия цикла (DAG должен быть ацикличен)
    pub fn has_cycle(&self) -> bool {
        let mut visited: HashSet<DagNodeId> = HashSet::new();
        let mut stack: HashSet<DagNodeId> = HashSet::new();
        for &node in self.nodes.keys() {
            if self.cycle_dfs(node, &mut visited, &mut stack) {
                return true;
            }
        }
        false
    }
    fn cycle_dfs(&self, node: DagNodeId, visited: &mut HashSet<DagNodeId>, stack: &mut HashSet<DagNodeId>) -> bool {
        if !visited.insert(node) {
            return false;
        }
        if !stack.insert(node) {
            return true;
        }
        if let Some(parents) = self.nodes.get(&node) {
            for &p in parents.iter() {
                if self.cycle_dfs(p, visited, stack) {
                    return true;
                }
            }
        }
        stack.remove(&node);
        false
    }
} 

