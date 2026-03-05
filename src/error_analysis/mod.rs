use ahash::AHashMap as HashMap;
use std::time::{SystemTime, Duration};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use log::{error, warn, info};
use crate::transaction::{Transaction, TransactionStatus};
// use crate::core::field::CoreState; // LEGACY: CoreState removed
use crate::core::consensus::ConsensusNode;
use crate::core::field::WaveValidationResult;
use smartcore::ensemble::random_forest_classifier::RandomForestClassifier;
use smartcore::ensemble::random_forest_classifier::RandomForestClassifierParameters;
use smartcore::linalg::basic::matrix::DenseMatrix;



#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Decentralized wavefield regeneration based on trusted scores per wave.
/// This function amplifies trusted waves and extends their lifetime via existing
/// WaveValidationResult mapping semantics (Validated/PartiallyValidated).
///
/// Inputs:
/// - consensus: mutable reference to the local consensus node (holds the wave field and engine)
/// - trust_map: map wave_id -> trust score in [0,1]
///
/// Behavior:
/// - trust >= 0.6 => Validated(trust) which amplifies wave and sets lifetime to 600s
/// - 0.3 <= trust < 0.6 => PartiallyValidated(trust*0.5) which attenuates less and sets 300s
/// - trust < 0.3 => ignored (no change)
///
/// Returns number of waves updated.
pub fn regenerate_wavefield(consensus: &mut ConsensusNode, trust_map: &std::collections::HashMap<String, f64>) -> usize {
    // Snapshot current waves to build a result map without holding inner locks too long
    let waves = consensus.field.snapshot_waves();
    if waves.is_empty() { return 0; }

    let mut updates: std::collections::HashMap<std::sync::Arc<str>, WaveValidationResult> =
        std::collections::HashMap::with_capacity(waves.len());

    for w in &waves {
        if let Some(score) = trust_map.get(&w.id.to_string()).copied() {
            let s = score.clamp(0.0, 1.0);
            if s >= 0.6 {
                updates.insert(std::sync::Arc::clone(&w.id), WaveValidationResult::Validated(s));
            } else if s >= 0.3 {
                updates.insert(std::sync::Arc::clone(&w.id), WaveValidationResult::PartiallyValidated(s * 0.5));
            }
        }
    }

    if updates.is_empty() { return 0; }

    // Apply amplitude/lifetime changes atomically for the batch
    consensus.field.update_wave_amplitudes(&updates);
    // Cleanup invalidated/extinguished waves after regeneration
    consensus.field.remove_extinguished_waves();
    consensus.field.cleanup_expired_waves();

    updates.len()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorCategory {
    // Сетевые ошибки
    NetworkConnection,
    NetworkAuthentication,
    NetworkProtocol,
    NetworkResource,
    NetworkConfiguration,

    // Квантовые ошибки
    CoreState,
    CoreMeasurement,
    CoreInterference,
    CoreEntanglement,
    CoreDecoherence,

    // Ошибки шардинга
    ShardAssignment,
    ShardSynchronization,
    ShardConsensus,
    ShardResource,

    // Ошибки консенсуса
    ConsensusProtocol,
    ConsensusValidation,
    ConsensusTimeout,
    ConsensusConflict,

    // Системные ошибки
    SystemResource,
    SystemConfiguration,
    SystemSecurity,
    SystemPerformance,

    // Неизвестные ошибки
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorType {
    TransactionError {
        transaction_id: String,
        status: TransactionStatus,
        Core_state: Option<String>, // Changed from Option<CoreState>
        error_message: String,
    },
    CoreError {
        state_id: String,
        amplitude: f64,
        expected_amplitude: f64,
        interference_score: f64,
    },
    SemanticError {
        context: String,
        expected_behavior: String,
        actual_behavior: String,
    },
    SystemError {
        component: String,
        error_code: String,
        details: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    pub timestamp: SystemTime,
    pub component_id: Option<Uuid>,
    pub related_component_id: Option<Uuid>,
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub description: String,
    pub stack_trace: Option<String>,
    pub system_state: HashMap<String, String>,
    pub Core_state: Option<HashMap<String, f64>>,
    pub shard_info: Option<HashMap<String, String>>,
    pub consensus_data: Option<HashMap<String, String>>,
    pub error_type: ErrorType,
    pub affected_components: Vec<String>,
    pub propagation_path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorAnalysis {
    pub error_id: Uuid,
    pub context: ErrorContext,
    pub root_cause: String,
    pub possible_solutions: Vec<String>,
    pub applied_solution: Option<String>,
    pub resolution_time: Option<Duration>,
    pub recurrence_count: u32,
    pub related_errors: Vec<Uuid>,
    pub impact_analysis: ImpactAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    pub affected_components: Vec<String>,
    pub performance_impact: f64,
    pub security_impact: f64,
    pub reliability_impact: f64,
    pub recovery_time: Option<Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemErrorAnalyzer {
    errors: HashMap<Uuid, ErrorAnalysis>,
    error_patterns: HashMap<String, Vec<String>>,
    solutions_history: HashMap<String, Vec<String>>,
    component_health: HashMap<String, f64>,
    system_metrics: SystemMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub total_errors: u32,
    pub resolved_errors: u32,
    pub average_resolution_time: Duration,
    pub system_health_score: f64,
    pub component_health_scores: HashMap<String, f64>,
}

pub struct ErrorAnalyzer {
    error_history: Vec<ErrorContext>,
    error_patterns: HashMap<String, Vec<ErrorType>>,
    Core_threshold: f64,
}

impl SystemErrorAnalyzer {
    pub fn new() -> Self {
        Self {
            errors: HashMap::new(),
            error_patterns: HashMap::new(),
            solutions_history: HashMap::new(),
            component_health: HashMap::new(),
            system_metrics: SystemMetrics {
                total_errors: 0,
                resolved_errors: 0,
                average_resolution_time: Duration::from_secs(0),
                system_health_score: 1.0,
                component_health_scores: HashMap::new(),
            },
        }
    }

    pub fn analyze_error(&mut self, context: ErrorContext) -> ErrorAnalysis {
        let error_id = Uuid::new_v4();
        
        // Анализ корневой причины
        let root_cause = self.determine_root_cause(&context);
        
        // Поиск возможных решений
        let possible_solutions = self.find_possible_solutions(&context, &root_cause);
        
        // Анализ влияния ошибки
        let impact_analysis = self.analyze_impact(&context, &root_cause);

        let analysis = ErrorAnalysis {
            error_id,
            context: context.clone(),
            root_cause,
            possible_solutions,
            applied_solution: None,
            resolution_time: None,
            recurrence_count: 0, // временно убираем анализ повторов
            related_errors: Vec::new(), // временно убираем связанные ошибки
            impact_analysis,
        };

        // Обновление метрик системы
        self.update_system_metrics(&analysis);
        
        // Логирование ошибки
        self.log_error(&analysis);

        self.errors.insert(error_id, analysis.clone());
        analysis
    }

    fn determine_root_cause(&self, context: &ErrorContext) -> String {
        match context.category {
            ErrorCategory::NetworkConnection => {
                if context.description.contains("Address already in use") {
                    "Порт уже используется другим процессом".to_string()
                } else if context.description.contains("Connection refused") {
                    "Удаленный узел недоступен или отклоняет соединение".to_string()
                } else {
                    "Неизвестная проблема с соединением".to_string()
                }
            },
            ErrorCategory::CoreState => {
                if let Some(state) = &context.Core_state {
                    if state.values().any(|&v| v.is_nan() || v.is_infinite()) {
                        "Некорректное квантовое состояние: обнаружены недопустимые значения".to_string()
                    } else {
                        "Нарушение квантового состояния".to_string()
                    }
                } else {
                    "Отсутствует информация о квантовом состоянии".to_string()
                }
            },
            ErrorCategory::ShardConsensus => {
                if let Some(shard_info) = &context.shard_info {
                    if shard_info.contains_key("conflict") {
                        "Конфликт консенсуса между шардами".to_string()
                    } else {
                        "Нарушение консенсуса в шарде".to_string()
                    }
                } else {
                    "Проблема с консенсусом шарда".to_string()
                }
            },
            // Добавьте обработку других категорий ошибок
            _ => "Неизвестная причина ошибки".to_string(),
        }
    }

    fn find_possible_solutions(&self, context: &ErrorContext, root_cause: &str) -> Vec<String> {
        let mut solutions = Vec::new();
        
        match context.category {
            ErrorCategory::NetworkConnection => {
                if root_cause.contains("порт уже используется") {
                    solutions.push("Освободить порт, завершив процесс, который его использует".to_string());
                    solutions.push("Изменить порт в конфигурации узла".to_string());
                    solutions.push("Добавить автоматический поиск свободного порта".to_string());
                }
            },
            ErrorCategory::CoreState => {
                solutions.push("Переинициализировать квантовое состояние".to_string());
                solutions.push("Применить квантовую коррекцию ошибок".to_string());
                solutions.push("Проверить и исправить параметры квантовых операций".to_string());
            },
            ErrorCategory::ShardConsensus => {
                solutions.push("Перезапустить процесс консенсуса".to_string());
                solutions.push("Проверить и синхронизировать состояние шардов".to_string());
                solutions.push("Применить механизм разрешения конфликтов".to_string());
            },
            // Добавьте решения для других категорий
            _ => {
                solutions.push("Собрать дополнительную диагностическую информацию".to_string());
                solutions.push("Проверить логи системы".to_string());
            },
        }

        solutions
    }

    fn analyze_impact(&self, context: &ErrorContext, root_cause: &str) -> ImpactAnalysis {
        let mut affected_components = Vec::new();
        let mut performance_impact = 0.0;
        let mut security_impact = 0.0;
        let mut reliability_impact = 0.0;

        match context.category {
            ErrorCategory::NetworkConnection => {
                affected_components.push("Network".to_string());
                performance_impact = 0.7;
                security_impact = 0.3;
                reliability_impact = 0.8;
            },
            ErrorCategory::CoreState => {
                affected_components.push("Core".to_string());
                affected_components.push("Consensus".to_string());
                performance_impact = 0.5;
                security_impact = 0.9;
                reliability_impact = 0.6;
            },
            ErrorCategory::ShardConsensus => {
                affected_components.push("Sharding".to_string());
                affected_components.push("Consensus".to_string());
                performance_impact = 0.8;
                security_impact = 0.4;
                reliability_impact = 0.9;
            },
            _ => {
                affected_components.push("System".to_string());
                performance_impact = 0.3;
                security_impact = 0.3;
                reliability_impact = 0.3;
            },
        }

        ImpactAnalysis {
            affected_components,
            performance_impact,
            security_impact,
            reliability_impact,
            recovery_time: None,
        }
    }

    fn update_system_metrics(&mut self, analysis: &ErrorAnalysis) {
        self.system_metrics.total_errors += 1;
        
        // Обновление здоровья компонентов
        for component in &analysis.impact_analysis.affected_components {
            let current_health = self.component_health.entry(component.clone())
                .or_insert(1.0);
            *current_health *= 1.0 - analysis.impact_analysis.reliability_impact;
        }

        // Обновление общего здоровья системы
        let avg_component_health: f64 = self.component_health.values().sum::<f64>() 
            / self.component_health.len() as f64;
        self.system_metrics.system_health_score = avg_component_health;
    }

    fn log_error(&self, analysis: &ErrorAnalysis) {
        match analysis.context.severity {
            ErrorSeverity::Critical => {
                error!(
                    "КРИТИЧЕСКАЯ ОШИБКА: {} (ID: {})\nПричина: {}\nВлияние: {:?}",
                    analysis.context.description,
                    analysis.error_id,
                    analysis.root_cause,
                    analysis.impact_analysis
                );
            },
            ErrorSeverity::High => {
                error!(
                    "СЕРЬЕЗНАЯ ОШИБКА: {} (ID: {})\nПричина: {}\nВлияние: {:?}",
                    analysis.context.description,
                    analysis.error_id,
                    analysis.root_cause,
                    analysis.impact_analysis
                );
            },
            ErrorSeverity::Medium => {
                warn!(
                    "ОШИБКА: {} (ID: {})\nПричина: {}\nВлияние: {:?}",
                    analysis.context.description,
                    analysis.error_id,
                    analysis.root_cause,
                    analysis.impact_analysis
                );
            },
            ErrorSeverity::Low => {
                info!(
                    "ПРЕДУПРЕЖДЕНИЕ: {} (ID: {})\nПричина: {}\nВлияние: {:?}",
                    analysis.context.description,
                    analysis.error_id,
                    analysis.root_cause,
                    analysis.impact_analysis
                );
            },
        }
    }

    pub fn get_system_health_report(&self) -> String {
        format!(
            "Состояние системы:\n\
             Общее здоровье: {:.2}%\n\
             Всего ошибок: {}\n\
             Решено ошибок: {}\n\
             Среднее время решения: {:?}\n\
             Здоровье компонентов:\n{}",
            self.system_metrics.system_health_score * 100.0,
            self.system_metrics.total_errors,
            self.system_metrics.resolved_errors,
            self.system_metrics.average_resolution_time,
            self.component_health.iter()
                .map(|(k, v)| format!("  {}: {:.2}%", k, v * 100.0))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

pub struct ErrorAI {
    model: RandomForestClassifier<f64, u32, DenseMatrix<f64>, Vec<u32>>,
}

impl ErrorAI {
    pub fn new() -> Self {
        let x = DenseMatrix::from_2d_vec(&vec![vec![0.6], vec![0.1]]);
        let y = vec![1, 0];
        let model = RandomForestClassifier::fit(
            &x,
            &y,
            RandomForestClassifierParameters::default()
        ).unwrap();
        Self { model }
    }

    pub fn predict_error(&self, features: Vec<f64>) -> u32 {
        let x = DenseMatrix::from_2d_vec(&vec![features]);
        self.model.predict(&x).unwrap()[0]
    }
}

impl ErrorAnalyzer {
    pub fn new() -> Self {
        Self {
            error_history: Vec::new(),
            error_patterns: HashMap::new(),
            Core_threshold: 0.5,
        }
    }

    pub fn analyze_transaction_error(&mut self, transaction: &Transaction, error: &str) -> ErrorContext {
        let error_type = ErrorType::TransactionError {
            transaction_id: transaction.id.to_string(),
            status: transaction.status.clone(),
            Core_state: None, // Будет заполнено позже при анализе
            error_message: error.to_string(),
        };

        let severity = self.determine_error_severity(&error_type);
        let affected_components = self.identify_affected_components(&error_type);
        let propagation_path = self.analyze_error_propagation(&error_type);

        let context = ErrorContext {
            timestamp: SystemTime::now(),
            error_type,
            severity,
            affected_components,
            propagation_path,
            component_id: None,
            related_component_id: None,
            category: ErrorCategory::Unknown,
            description: error.to_string(),
            stack_trace: None,
            system_state: HashMap::new(),
            Core_state: None,
            shard_info: None,
            consensus_data: None,
        };

        self.error_history.push(context.clone());
        self.update_error_patterns(&context);

        context
    }

    /* LEGACY: CoreState removed
    pub fn analyze_Core_state(&mut self, state: &CoreState) -> Option<ErrorContext> {
        // Проверяем амплитуду квантового состояния
        if state.amplitude < self.Core_threshold {
            let error_type = ErrorType::CoreError {
                state_id: state.id.clone(),
                amplitude: state.amplitude,
                expected_amplitude: self.Core_threshold,
                interference_score: self.calculate_interference_score(state),
            };

            let severity = self.determine_error_severity(&error_type);
            let affected_components = self.identify_affected_components(&error_type);
            let propagation_path = self.analyze_error_propagation(&error_type);

            let context = ErrorContext {
                timestamp: SystemTime::now(),
                error_type,
                severity,
                affected_components,
                propagation_path,
                component_id: None,
                related_component_id: None,
                category: ErrorCategory::CoreState,
                description: format!("Некорректное квантовое состояние: амплитуда {} < {}", state.amplitude, self.Core_threshold),
                stack_trace: None,
                system_state: HashMap::new(),
                Core_state: Some(state.superposition.iter().enumerate().map(|(i, vector): (usize, &crate::core::field::StateVector)| (i.to_string(), vector.probability)).collect()),
                shard_info: None,
                consensus_data: None,
            };

            self.error_history.push(context.clone());
            self.update_error_patterns(&context);

            Some(context)
        } else {
            None
        }
    }
    */

    fn determine_error_severity(&self, error_type: &ErrorType) -> ErrorSeverity {
        match error_type {
            ErrorType::TransactionError { status, .. } => {
                match status {
                    TransactionStatus::Failed => ErrorSeverity::High,
                    TransactionStatus::Processing => ErrorSeverity::Medium,
                    _ => ErrorSeverity::Low,
                }
            },
            ErrorType::CoreError { amplitude, .. } => {
                if *amplitude < 0.3 {
                    ErrorSeverity::Critical
                } else if *amplitude < 0.5 {
                    ErrorSeverity::High
                } else {
                    ErrorSeverity::Medium
                }
            },
            ErrorType::SemanticError { .. } => ErrorSeverity::High,
            ErrorType::SystemError { .. } => ErrorSeverity::Critical,
        }
    }

    fn identify_affected_components(&self, error_type: &ErrorType) -> Vec<String> {
        match error_type {
            ErrorType::TransactionError { .. } => {
                vec!["transaction_processor".to_string(), "Core_field".to_string()]
            },
            ErrorType::CoreError { .. } => {
                vec!["Core_field".to_string(), "interference_engine".to_string()]
            },
            ErrorType::SemanticError { .. } => {
                vec!["semantic_engine".to_string(), "transaction_processor".to_string()]
            },
            ErrorType::SystemError { component, .. } => {
                vec![component.clone()]
            },
        }
    }

    fn analyze_error_propagation(&self, error_type: &ErrorType) -> Vec<String> {
        match error_type {
            ErrorType::TransactionError { .. } => {
                vec![
                    "transaction_processor".to_string(),
                    "Core_field".to_string(),
                    "consensus_engine".to_string(),
                ]
            },
            ErrorType::CoreError { .. } => {
                vec![
                    "Core_field".to_string(),
                    "interference_engine".to_string(),
                    "transaction_processor".to_string(),
                ]
            },
            ErrorType::SemanticError { .. } => {
                vec![
                    "semantic_engine".to_string(),
                    "transaction_processor".to_string(),
                    "Core_field".to_string(),
                ]
            },
            ErrorType::SystemError { .. } => {
                vec!["system_core".to_string()]
            },
        }
    }

    /* LEGACY: CoreState removed
    fn calculate_interference_score(&self, state: &CoreState) -> f64 {
        // Простой расчет интерференционного скора на основе суперпозиции
        state.superposition.iter()
            .map(|vector| vector.probability)
            .sum::<f64>() / state.superposition.len() as f64
    }
    */

    fn update_error_patterns(&mut self, context: &ErrorContext) {
        let pattern_key = self.generate_pattern_key(&context.error_type);
        let patterns = self.error_patterns.entry(pattern_key)
            .or_insert_with(Vec::new);
        patterns.push(context.error_type.clone());
    }

    fn generate_pattern_key(&self, error_type: &ErrorType) -> String {
        match error_type {
            ErrorType::TransactionError { status, .. } => format!("tx_{:?}", status),
            ErrorType::CoreError { .. } => "Core".to_string(),
            ErrorType::SemanticError { .. } => "semantic".to_string(),
            ErrorType::SystemError { component, .. } => format!("system_{}", component),
        }
    }

    pub fn get_error_statistics(&self) -> HashMap<String, usize> {
        let mut stats = HashMap::new();
        for context in &self.error_history {
            let key = self.generate_pattern_key(&context.error_type);
            *stats.entry(key).or_insert(0) += 1;
        }
        stats
    }

    pub fn ai_predict_error(&self, context: ErrorContext, ai: Option<&ErrorAI>) -> ErrorType {
        if let Some(ai) = ai {
            // Пример: используем Random Forest для предсказания вероятности CoreError
            let features = vec![context.severity as u8 as f32]; // Можно расширить набор признаков
            let score = ai.predict_error(features.iter().map(|&f| f as f64).collect());
            if score == 1 {
                ErrorType::CoreError {
                    state_id: String::new(),
                    amplitude: 0.0,
                    expected_amplitude: 0.0,
                    interference_score: 0.0,
                }
            } else {
                ErrorType::SystemError {
                    component: String::new(),
                    error_code: String::new(),
                    details: String::new(),
                }
            }
        } else {
            // Placeholder ML model
            if context.description.contains("Core") {
                ErrorType::CoreError {
                    state_id: "predicted".to_string(),
                    amplitude: 0.5,
                    expected_amplitude: 1.0,
                    interference_score: 0.3,
                }
            } else {
                ErrorType::SystemError {
                    component: String::new(),
                    error_code: String::new(),
                    details: String::new(),
                }
            }
        }
    }

    pub fn analyze_error(&mut self, error_data: &[f32]) -> Result<ErrorType, String> {
        // Используем Random Forest вместо CatBoost
        let x = DenseMatrix::from_2d_vec(&vec![error_data.to_vec()]);
        
        // В реальном коде здесь должна быть загрузка предобученной модели
        let model = RandomForestClassifier::fit(
            &x,
            &vec![0; error_data.len()],  // dummy labels для примера
            Default::default()
        ).map_err(|e| e.to_string())?;
        
        let prediction = model.predict(&x).map_err(|e| e.to_string())?;
        
        // Преобразуем предсказание в ErrorType
        match prediction[0] {
            0 => Ok(ErrorType::TransactionError {
                transaction_id: "".to_string(),
                status: TransactionStatus::Failed,
                Core_state: None,
                error_message: "Predicted error".to_string()
            }),
            _ => Ok(ErrorType::SystemError {
                component: "unknown".to_string(),
                error_code: "0".to_string(),
                details: "Unknown error".to_string()
            })
        }
    }
} 


