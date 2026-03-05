use super::{Transaction, TransactionStorage};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;
use lazy_static::lazy_static;

lazy_static! {
    static ref TRANSACTION_STORAGE: Mutex<HashMap<String, Transaction>> = Mutex::new(HashMap::new());
}

/// Реализация хранилища транзакций в памяти
pub struct InMemoryTransactionStorage;

impl InMemoryTransactionStorage {
    pub fn new() -> Self {
        Self
    }
}

impl TransactionStorage for InMemoryTransactionStorage {
    fn save(&self, transaction: &Transaction) -> Result<(), String> {
        let mut storage = TRANSACTION_STORAGE.lock().unwrap();
        storage.insert(transaction.id.to_string(), transaction.clone());
        Ok(())
    }

    fn get(&self, id: &Uuid) -> Option<Transaction> {
        let storage = TRANSACTION_STORAGE.lock().unwrap();
        storage.get(&id.to_string()).cloned()
    }

    fn update(&self, transaction: &Transaction) -> Result<(), String> {
        let mut storage = TRANSACTION_STORAGE.lock().unwrap();
        if storage.contains_key(&transaction.id.to_string()) {
            storage.insert(transaction.id.to_string(), transaction.clone());
            Ok(())
        } else {
            Err("Транзакция не найдена".to_string())
        }
    }

    fn delete(&self, id: &Uuid) -> Result<(), String> {
        let mut storage = TRANSACTION_STORAGE.lock().unwrap();
        if storage.remove(&id.to_string()).is_some() {
            Ok(())
        } else {
            Err("Транзакция не найдена".to_string())
        }
    }
} 

