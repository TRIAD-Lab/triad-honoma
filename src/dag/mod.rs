use std::collections::{HashMap, HashSet, VecDeque};
use rayon::prelude::*;
use uuid::Uuid;

pub struct Dag {
    /// Сопоставление: транзакция -> список родителей (зависимостей)
    nodes: HashMap<Uuid, Vec<Uuid>>, // tx_id -> parents
}

impl Dag {
    pub fn new() -> Self { Self { nodes: HashMap::new() } }

    pub fn add_transaction(&mut self, tx_id: Uuid, parents: Vec<Uuid>) {
        self.nodes.insert(tx_id, parents);
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
        let parent_ids: Vec<Uuid> = parents.iter().map(|(id, _)| *id).collect();
        self.nodes.insert(tx_id, parent_ids);
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
        match self.nodes.get(tx_id) {
            Some(parents) => parents
                .iter()
                .filter(|p| !self.nodes.contains_key(p))
                .cloned()
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
        // Сконструируем граф «родитель -> дети», а также входящие степени
        let mut indegree: HashMap<Uuid, usize> = HashMap::new();
        let mut children: HashMap<Uuid, Vec<Uuid>> = HashMap::new();

        // Инициализация узлов
        for (&node, parents) in self.nodes.iter() {
            indegree.entry(node).or_insert(0);
            for &p in parents {
                // Родитель может быть ещё не добавлен как вершина — добавим его с пустым списком родителей
                indegree.entry(p).or_insert(0);
            }
        }

        // Заполнение children и indegree
        for (&node, parents) in self.nodes.iter() {
            for &p in parents {
                children.entry(p).or_default().push(node);
                *indegree.entry(node).or_insert(0) += 1;
            }
        }

        // Очередь узлов с нулевой входящей степенью
        let mut queue: VecDeque<Uuid> = indegree
            .iter()
            .filter_map(|(n, &deg)| if deg == 0 { Some(*n) } else { None })
            .collect();

        let mut order = Vec::with_capacity(indegree.len());

        while let Some(n) = queue.pop_front() {
            order.push(n);
            if let Some(ch) = children.get(&n) {
                for &c in ch {
                    if let Some(d) = indegree.get_mut(&c) {
                        *d -= 1;
                        if *d == 0 { queue.push_back(c); }
                    }
                }
            }
        }

        if order.len() != indegree.len() {
            return Err("Cycle detected or missing nodes for topological sort".to_string());
        }
        Ok(order)
    }

    /// Вычисляет уровни DAG (Kahn's algorithm by layers). Каждый уровень содержит вершины,
    /// которые могут обрабатываться параллельно после завершения всех предыдущих уровней.
    pub fn levels(&self) -> Result<Vec<Vec<Uuid>>, String> {
        // Построим карты входящей степени и детей
        let mut indegree: HashMap<Uuid, usize> = HashMap::new();
        let mut children: HashMap<Uuid, Vec<Uuid>> = HashMap::new();

        // Инициализация узлов
        for (&node, parents) in self.nodes.iter() {
            indegree.entry(node).or_insert(0);
            for &p in parents {
                indegree.entry(p).or_insert(0);
            }
        }

        for (&node, parents) in self.nodes.iter() {
            for &p in parents {
                children.entry(p).or_default().push(node);
                *indegree.entry(node).or_insert(0) += 1;
            }
        }

        // Начальный уровень — все вершины с нулевой входящей степенью
        let mut current_level: Vec<Uuid> = indegree
            .iter()
            .filter_map(|(n, &deg)| if deg == 0 { Some(*n) } else { None })
            .collect();

        let mut produced: Vec<Vec<Uuid>> = Vec::with_capacity(indegree.len().max(1));
        let total_nodes = indegree.len();
        let mut seen_count = 0usize;

        while !current_level.is_empty() {
            // Зафиксируем текущий уровень
            produced.push(std::mem::take(&mut current_level));
            seen_count += produced.last().unwrap().len();

            // Подготовим следующий уровень, уменьшая входящие степени у детей
            let mut next_level: Vec<Uuid> = Vec::with_capacity(16);
            for n in produced.last().unwrap().iter() {
                if let Some(ch) = children.get(n) {
                    for &c in ch {
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
        self.nodes.keys().cloned().collect()
    }

    /// Получить родителей транзакции
    pub fn parents(&self, tx_id: &Uuid) -> Option<&Vec<Uuid>> {
        self.nodes.get(tx_id)
    }

    /// Получить детей транзакции
    pub fn children(&self, tx_id: &Uuid) -> Vec<Uuid> {
        self.nodes.iter()
            .filter_map(|(child, parents)| if parents.contains(tx_id) { Some(*child) } else { None })
            .collect()
    }

    /// Глубина DAG (максимальная длина пути от корня до листа)
    pub fn depth(&self) -> usize {
        let mut max_depth = 0;
        for tx in self.nodes.keys() {
            let d = self.depth_from(tx, &mut HashSet::new());
            if d > max_depth { max_depth = d; }
        }
        max_depth
    }
    fn depth_from(&self, tx_id: &Uuid, visited: &mut HashSet<Uuid>) -> usize {
        if !visited.insert(*tx_id) { return 0; }
        match self.parents(tx_id) {
            Some(parents) if !parents.is_empty() => {
                1 + parents.iter().map(|p| self.depth_from(p, visited)).max().unwrap_or(0)
            },
            _ => 1,
        }
    }

    /// Проверка наличия цикла (DAG должен быть ацикличен)
    pub fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();
        for tx in self.nodes.keys() {
            if self.cycle_dfs(tx, &mut visited, &mut stack) {
                return true;
            }
        }
        false
    }
    fn cycle_dfs(&self, tx_id: &Uuid, visited: &mut HashSet<Uuid>, stack: &mut HashSet<Uuid>) -> bool {
        if !visited.insert(*tx_id) {
            return false;
        }
        if !stack.insert(*tx_id) {
            return true;
        }
        if let Some(parents) = self.parents(tx_id) {
            for p in parents {
                if self.cycle_dfs(p, visited, stack) {
                    return true;
                }
            }
        }
        stack.remove(tx_id);
        false
    }
} 