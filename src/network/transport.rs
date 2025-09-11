use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use uuid::Uuid;
use log::{debug, error, info, warn};
use quinn::Endpoint;

use crate::network::error::NetworkError;
use crate::network::types::{Message, NetworkEvent, PeerInfo};
use crate::network::peer::PeerManager;


/// Максимальный размер сообщения (10 МБ)
const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;
/// Размер буфера для чтения
const READ_BUFFER_SIZE: usize = 8192;

/// Сервис транспорта для обмена сообщениями
#[derive(Debug)]
pub struct TransportService {
    /// ID текущего узла
    node_id: Uuid,
    /// Порт для TCP-соединений
    port: u16,
    /// Канал для отправки событий сети
    event_sender: mpsc::Sender<NetworkEvent>,
    /// Менеджер узлов
    peer_manager: Arc<RwLock<PeerManager>>,
    /// Активные соединения с узлами
    connections: Arc<Mutex<HashMap<Uuid, mpsc::Sender<Message>>>>,
    /// Флаг, указывающий, запущен ли сервис
    running: bool,
}

impl TransportService {
    /// Создаёт новый транспортный сервис
    pub fn new(
        node_id: Uuid,
        port: u16,
        event_sender: mpsc::Sender<NetworkEvent>,
        peer_manager: Arc<RwLock<PeerManager>>,
    ) -> Self {
        Self {
            node_id,
            port,
            event_sender,
            peer_manager,
            connections: Arc::new(Mutex::new(HashMap::new())),
            running: false,
        }
    }
    
    /// Запускает транспортный сервис
    pub async fn start(&self) -> Result<(), NetworkError> {
        if self.running {
            return Err(NetworkError::ServiceAlreadyStarted);
        }
        
        // Создаём TCP-слушатель
        let listener = match TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], self.port))).await {
            Ok(listener) => listener,
            Err(e) => {
                error!("Не удалось создать TCP-слушатель на порту {}: {}", self.port, e);
                return Err(NetworkError::Io(e));
            }
        };
        
        let connections = self.connections.clone();
        let event_sender = self.event_sender.clone();
        let peer_manager = self.peer_manager.clone();
        let node_id = self.node_id;
        
        // Запускаем прослушивание входящих соединений
        tokio::spawn(async move {
            info!("Запущен TCP-слушатель на порту {}", listener.local_addr().unwrap().port());
            
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!("Принято входящее соединение от {}", addr);
                        
                        // Находим узел по адресу
                        let peer_id = {
                            let manager = peer_manager.read().await;
                            match manager.find_peer_by_address(&addr) {
                                Some(peer) => peer.id,
                                None => {
                                    warn!("Получено соединение от неизвестного узла: {}", addr);
                                    // В реальном приложении здесь можно было бы запросить идентификацию
                                    continue;
                                }
                            }
                        };
                        
                        // Создаём канал для отправки сообщений этому узлу
                        let (tx, mut rx) = mpsc::channel::<Message>(100);
                        
                        // Добавляем отправитель в список активных соединений
                        {
                            let mut conns = connections.lock().await;
                            conns.insert(peer_id, tx);
                        }
                        
                        // Обновляем время последнего взаимодействия
                        {
                            let mut manager = peer_manager.write().await;
                            if let Err(e) = manager.update_last_seen(&peer_id) {
                                error!("Ошибка обновления времени последнего взаимодействия: {}", e);
                            }
                        }
                        
                        // Создаем второй сокет через дупликацию файлового дескриптора
                        // для отправки сообщений в отдельной задаче
                        let writer_stream = match create_writer_stream(&stream).await {
                            Ok(stream) => stream,
                            Err(e) => {
                                error!("Ошибка создания потока для записи: {}", e);
                                continue;
                            }
                        };
                        
                        // Запускаем задачу для отправки сообщений узлу
                        tokio::spawn(async move {
                            let mut writer = writer_stream;
                            while let Some(message) = rx.recv().await {
                                // Сериализуем сообщение
                                match bincode::serialize(&message) {
                                    Ok(message_bytes) => {
                                        // Проверяем размер сообщения
                                        if message_bytes.len() > MAX_MESSAGE_SIZE {
                                            error!("Слишком большое сообщение для отправки узлу {}: {} байт",
                                                peer_id, message_bytes.len());
                                            continue;
                                        }
                                        
                                        // Сначала отправляем размер сообщения (4 байта)
                                        let message_len = (message_bytes.len() as u32).to_be_bytes();
                                        if let Err(e) = writer.write_all(&message_len).await {
                                            error!("Ошибка отправки размера сообщения узлу {}: {}", peer_id, e);
                                            break;
                                        }
                                        
                                        // Затем отправляем само сообщение
                                        if let Err(e) = writer.write_all(&message_bytes).await {
                                            error!("Ошибка отправки сообщения узлу {}: {}", peer_id, e);
                                            break;
                                        }
                                        
                                        debug!("Отправлено сообщение узлу {}: тип={:?}, id={}",
                                            peer_id, message.message_type, message.id);
                                    },
                                    Err(e) => {
                                        error!("Ошибка сериализации сообщения для узла {}: {}", peer_id, e);
                                    }
                                }
                            }
                            
                            info!("Закрыт канал отправки сообщений для узла {}", peer_id);
                        });
                        
                        // Запускаем обработку входящих сообщений от этого узла
                        let conn_event_sender = event_sender.clone();
                        let conn_peer_manager = peer_manager.clone();
                        
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(
                                stream,
                                peer_id,
                                node_id,
                                conn_event_sender,
                                conn_peer_manager,
                            ).await {
                                error!("Ошибка обработки соединения с узлом {}: {}", peer_id, e);
                            }
                        });
                    },
                    Err(e) => {
                        error!("Ошибка принятия TCP-соединения: {}", e);
                    }
                }
            }
        });
        
        info!("Запущен транспортный сервис на порту {}", self.port);
        
        Ok(())
    }
    
    /// Останавливает транспортный сервис
    pub async fn stop(&self) -> Result<(), NetworkError> {
        // В реальной реализации здесь нужно остановить все задачи и закрыть соединения
        // Для простоты примера этот метод пока ничего не делает
        
        info!("Остановлен транспортный сервис");
        
        Ok(())
    }
    
    /// Отправляет сообщение узлу
    pub async fn send_message(&self, peer_id: Uuid, message: Message) -> Result<(), NetworkError> {
        // Получаем отправитель сообщений для узла или создаём новое соединение
        let sender = self.get_or_create_connection(peer_id).await?;
        
        // Сохраняем нужные поля для логирования до перемещения message
        let msg_type = message.message_type.clone();
        let msg_id = message.id;
        
        // Отправляем сообщение через канал (передаём владение без clone)
        if let Err(_) = sender.send(message).await {
            return Err(NetworkError::ConnectionFailed(format!("Канал закрыт для узла {}", peer_id)));
        }
        
        // Обновляем время последнего взаимодействия
        {
            let mut manager = self.peer_manager.write().await;
            if let Err(e) = manager.update_last_seen(&peer_id) {
                warn!("Ошибка обновления времени последнего взаимодействия: {}", e);
            }
        }
        
        debug!("Сообщение помещено в очередь для отправки узлу {}: тип={:?}, id={}",
            peer_id, msg_type, msg_id);
        
        Ok(())
    }
    
    /// Возвращает список активных узлов
    pub async fn get_active_peers(&self) -> Result<Vec<PeerInfo>, NetworkError> {
        let manager = self.peer_manager.read().await;
        Ok(manager.get_active_peers())
    }

    pub async fn send_quic(&self, data: Vec<u8>) {
        // Пример адреса: "127.0.0.1:5000"
        let server_addr = "127.0.0.1:5000".parse().unwrap();
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let conn = endpoint.connect(server_addr, "localhost").unwrap().await.unwrap();
        let mut stream = conn.open_uni().await.unwrap();
        stream.write_all(&data).await.unwrap();
        stream.finish().await.unwrap();
    }
    
    /// Обрабатывает входящее соединение
    async fn handle_connection(
        mut stream: TcpStream,
        peer_id: Uuid,
        _node_id: Uuid,
        event_sender: mpsc::Sender<NetworkEvent>,
        peer_manager: Arc<RwLock<PeerManager>>,
    ) -> Result<(), NetworkError> {
        let _buffer = vec![0u8; READ_BUFFER_SIZE];
        let mut size_buffer = [0u8; 4];
        
        loop {
            // Читаем размер сообщения
            match stream.read_exact(&mut size_buffer).await {
                Ok(_) => {
                    let message_size = u32::from_be_bytes(size_buffer) as usize;
                    
                    // Проверяем размер сообщения
                    if message_size > MAX_MESSAGE_SIZE {
                        error!("Получено слишком большое сообщение от узла {}: {} байт", peer_id, message_size);
                        return Err(NetworkError::Internal(format!(
                            "Размер сообщения превышает максимально допустимый ({} > {})",
                            message_size,
                            MAX_MESSAGE_SIZE
                        )));
                    }
                    
                    // Подготавливаем буфер для сообщения
                    let mut message_buffer = vec![0u8; message_size];
                    
                    // Читаем сообщение
                    match stream.read_exact(&mut message_buffer).await {
                        Ok(_) => {
                            // Десериализуем сообщение
                            match bincode::deserialize::<Message>(&message_buffer) {
                                Ok(message) => {
                                    // Обновляем время последнего взаимодействия
                                    {
                                        let mut manager = peer_manager.write().await;
                                        if let Err(e) = manager.update_last_seen(&peer_id) {
                                            warn!("Ошибка обновления времени последнего взаимодействия: {}", e);
                                        }
                                    }
                                    
                                    debug!("Получено сообщение от узла {}: тип={:?}, id={}",
                                        peer_id, message.message_type, message.id);
                                    
                                    // Отправляем событие о получении сообщения
                                    if let Err(e) = event_sender.send(NetworkEvent::MessageReceived {
                                        from: peer_id,
                                        message,
                                    }).await {
                                        error!("Ошибка отправки события MessageReceived: {}", e);
                                    }
                                },
                                Err(e) => {
                                    error!("Ошибка десериализации сообщения от узла {}: {}", peer_id, e);
                                }
                            }
                        },
                        Err(e) => {
                            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                                info!("Соединение с узлом {} закрыто", peer_id);
                                
                                // Отправляем событие об отключении узла
                                if let Err(e) = event_sender.send(NetworkEvent::PeerDisconnected(peer_id)).await {
                                    error!("Ошибка отправки события PeerDisconnected: {}", e);
                                }
                                
                                return Ok(());
                            } else {
                                error!("Ошибка чтения сообщения от узла {}: {}", peer_id, e);
                                return Err(NetworkError::Io(e));
                            }
                        }
                    }
                },
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        info!("Соединение с узлом {} закрыто", peer_id);
                        
                        // Отправляем событие об отключении узла
                        if let Err(e) = event_sender.send(NetworkEvent::PeerDisconnected(peer_id)).await {
                            error!("Ошибка отправки события PeerDisconnected: {}", e);
                        }
                        
                        return Ok(());
                    } else {
                        error!("Ошибка чтения размера сообщения от узла {}: {}", peer_id, e);
                        return Err(NetworkError::Io(e));
                    }
                }
            }
        }
    }
    
    /// Получает существующий отправитель сообщений для узла или создаёт новое соединение
    async fn get_or_create_connection(&self, peer_id: Uuid) -> Result<mpsc::Sender<Message>, NetworkError> {
        // Проверяем, есть ли уже соединение
        {
            let connections = self.connections.lock().await;
            if let Some(sender) = connections.get(&peer_id) {
                return Ok(sender.clone());
            }
        }
        
        // Получаем адрес узла
        let peer_addr = {
            let manager = self.peer_manager.read().await;
            manager.get_peer_address(&peer_id)?
        };
        
        // Устанавливаем соединение
        match TcpStream::connect(peer_addr).await {
            Ok(stream) => {
                info!("Установлено новое соединение с узлом {} ({})", peer_id, peer_addr);
                
                // Создаём канал для отправки сообщений
                let (tx, mut rx) = mpsc::channel::<Message>(100);
                
                // Клонируем отправитель для возврата
                let sender = tx.clone();
                
                // Добавляем отправитель в список активных соединений
                {
                    let mut connections = self.connections.lock().await;
                    connections.insert(peer_id, tx);
                }
                
                // Создаем второй сокет через дупликацию файлового дескриптора
                // для отправки сообщений
                let writer_stream = match create_writer_stream(&stream).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        error!("Ошибка создания потока для записи: {}", e);
                        return Err(NetworkError::Io(e));
                    }
                };
                
                // Запускаем задачу для отправки сообщений узлу
                tokio::spawn(async move {
                    let mut writer = writer_stream;
                    while let Some(message) = rx.recv().await {
                        // Сериализуем сообщение
                        match bincode::serialize(&message) {
                            Ok(message_bytes) => {
                                // Проверяем размер сообщения
                                if message_bytes.len() > MAX_MESSAGE_SIZE {
                                    error!("Слишком большое сообщение для отправки узлу {}: {} байт",
                                        peer_id, message_bytes.len());
                                    continue;
                                }
                                
                                // Сначала отправляем размер сообщения (4 байта)
                                let message_len = (message_bytes.len() as u32).to_be_bytes();
                                if let Err(e) = writer.write_all(&message_len).await {
                                    error!("Ошибка отправки размера сообщения узлу {}: {}", peer_id, e);
                                    break;
                                }
                                
                                // Затем отправляем само сообщение
                                if let Err(e) = writer.write_all(&message_bytes).await {
                                    error!("Ошибка отправки сообщения узлу {}: {}", peer_id, e);
                                    break;
                                }
                                
                                debug!("Отправлено сообщение узлу {}: тип={:?}, id={}",
                                    peer_id, message.message_type, message.id);
                            },
                            Err(e) => {
                                error!("Ошибка сериализации сообщения для узла {}: {}", peer_id, e);
                            }
                        }
                    }
                    
                    info!("Закрыт канал отправки сообщений для узла {}", peer_id);
                });
                
                // Запускаем обработку входящих сообщений
                let event_sender = self.event_sender.clone();
                let peer_manager = self.peer_manager.clone();
                let node_id = self.node_id;
                
                tokio::spawn(async move {
                    if let Err(e) = Self::handle_connection(
                        stream,
                        peer_id,
                        node_id,
                        event_sender,
                        peer_manager,
                    ).await {
                        error!("Ошибка обработки соединения с узлом {}: {}", peer_id, e);
                    }
                });
                
                Ok(sender)
            },
            Err(e) => {
                error!("Не удалось установить соединение с узлом {} ({}): {}", peer_id, peer_addr, e);
                Err(NetworkError::ConnectionFailed(format!("{}:{}", peer_addr, e)))
            }
        }
    }
}

/// Создает второй сокет через дупликацию файлового дескриптора
async fn create_writer_stream(stream: &TcpStream) -> std::io::Result<TcpStream> {
    // Получаем адрес, к которому подключен сокет
    let addr = stream.peer_addr()?;
    
    // Создаем новое соединение к тому же адресу
    TcpStream::connect(addr).await
} 