use std::time::SystemTime;
use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Статус транзакции
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Critical,
    High,
    Medium,
    Low,
}

/// Базовая структура транзакции
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: Uuid,
    pub sender: String,
    pub receiver: String,
    pub amount: f64,
    pub timestamp: SystemTime,
    pub status: TransactionStatus,
    pub data: Vec<u8>,
    pub signature: Vec<u8>, // Added for Schnorr signature
    pub parent_ids: Vec<String>, // Added for DAG
}

impl Transaction {
    /// Создает новую транзакцию
    pub fn new(sender: String, receiver: String, amount: f64, data: Vec<u8>) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: SystemTime::now(),
            sender,
            receiver,
            amount,
            status: TransactionStatus::Pending,
            data,
            signature: Vec::new(),
            parent_ids: Vec::new(),
        }
    }

    /// Обновляет статус транзакции
    pub fn update_status(&mut self, status: TransactionStatus) {
        self.status = status;
    }

    /// Проверяет валидность транзакции
    pub fn is_valid(&self) -> bool {
        self.amount > 0.0 && !self.sender.is_empty() && !self.receiver.is_empty()
    }

    /// Phase 3: Zero-copy serialization with postcard
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        postcard::to_stdvec(self).map_err(|e| format!("Serialization error: {}", e))
    }

    /// Phase 3: Zero-copy deserialization with postcard
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        postcard::from_bytes(bytes).map_err(|e| format!("Deserialization error: {}", e))
    }
}

/// Трейт для обработки транзакций
pub trait TransactionProcessor {
    fn process(&self, transaction: &mut Transaction) -> Result<(), String>;
    fn process_batch(&self, transactions: &mut [Transaction]) -> Result<(), String>;
    fn verify(&self, transaction: &Transaction) -> bool;
    fn create_wave_from_transaction(&self, transaction: &Transaction) -> crate::core::field::CoreWave;
    fn create_probabilistic_decision(&self, analysis: &crate::core::interference::InterferenceAnalysis, transactions: &[Transaction]) -> crate::core::prob_ops::ProbabilisticOperation;
    fn calculate_transaction_quality(&self, transactions: &[Transaction]) -> f64;
    fn calculate_ttl_factor(&self) -> f64;
    fn calculate_sender_reputation(&self, transactions: &[Transaction]) -> f64;
    fn create_verification_decision(&self, analysis: &crate::core::interference::InterferenceAnalysis, transaction: &Transaction) -> crate::core::prob_ops::ProbabilisticOperation;
    fn default_batch_context(&self) -> crate::core::consensus::BatchContextHandle;
}

/// Трейт для хранения транзакций
pub trait TransactionStorage {
    fn save(&self, transaction: &Transaction) -> Result<(), String>;
    fn get(&self, id: &Uuid) -> Option<Transaction>;
    fn update(&self, transaction: &Transaction) -> Result<(), String>;
    fn delete(&self, id: &Uuid) -> Result<(), String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartContract {
    pub id: Uuid,
    pub creator: String,
    pub code: Vec<u8>,
    pub state: Vec<u8>,
    pub status: ContractStatus,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractStatus {
    Pending,
    Deployed,
    Failed,
    Executed,
    Aborted,
}

pub trait SmartContractProcessor {
    fn deploy(&self, contract: &mut SmartContract) -> Result<(), String>;
    fn execute(&self, contract: &mut SmartContract, input: Vec<u8>) -> Result<Vec<u8>, String>;
    fn verify(&self, contract: &SmartContract) -> bool;
}

pub mod processor;
pub mod payload; 


