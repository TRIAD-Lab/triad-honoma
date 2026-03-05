use thiserror::Error;
use std::io;
use uuid::Uuid;

/// Ошибки сетевого модуля
#[derive(Debug, Error)]
pub enum NetworkError {
    /// Ошибка ввода-вывода
    #[error("Ошибка ввода-вывода: {0}")]
    Io(#[from] io::Error),
    
    /// Ошибка сериализации
    #[error("Ошибка сериализации: {0}")]
    Serialization(String),
    
    /// Ошибка десериализации
    #[error("Ошибка десериализации: {0}")]
    Deserialization(String),
    
    /// Узел не найден
    #[error("Узел не найден: {0}")]
    PeerNotFound(Uuid),
    
    /// Невозможно подключиться к узлу
    #[error("Невозможно подключиться к узлу: {0}")]
    ConnectionFailed(String),
    
    /// Узел отключен
    #[error("Узел отключен: {0}")]
    PeerDisconnected(Uuid),
    
    /// Таймаут операции
    #[error("Таймаут операции")]
    Timeout,
    
    /// Сервис уже запущен
    #[error("Сервис уже запущен")]
    ServiceAlreadyStarted,
    
    /// Сервис не запущен
    #[error("Сервис не запущен")]
    ServiceNotStarted,
    
    /// Внутренняя ошибка
    #[error("Внутренняя ошибка: {0}")]
    Internal(String),

    /// Ошибка остановки сервиса
    #[error("Не удалось корректно остановить сервис")]
    ShutdownFailed,
}

impl From<bincode::Error> for NetworkError {
    fn from(err: bincode::Error) -> Self {
        NetworkError::Serialization(err.to_string())
    }
}

impl From<serde_json::Error> for NetworkError {
    fn from(err: serde_json::Error) -> Self {
        NetworkError::Serialization(err.to_string())
    }
} 

