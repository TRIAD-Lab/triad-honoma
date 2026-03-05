use crate::{
    transaction::{Transaction, TransactionProcessor, TransactionStorage, SmartContract, CoreSmartContractProcessor, ContractStatus},
    Core::consensus::{ConsensusNode, ConsensusMessage, ConsensusState},
    error_analysis::{ErrorAnalyzer, ErrorContext},
    semantic::{SemanticEngine, SemanticAction},
    sharding::ShardEvent,
};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::SystemTime;
use serde_json;
use bytes::Bytes;

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub id: String,
    pub address: String,
    pub peers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NetworkMessage {
    pub message_type: MessageType,
    pub sender: String,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone)]
pub enum MessageType {
    Transaction(Transaction),
    Consensus(ConsensusMessage),
    Error(String),
    SmartContract(SmartContract),
    ShardEvent(ShardEvent),
    ErrorEvent(ErrorContext),
    SemanticAction(SemanticAction),
}

pub struct NetworkNode {
    config: NodeConfig,
    consensus: Arc<Mutex<ConsensusNode>>,
    error_analyzer: Arc<Mutex<ErrorAnalyzer>>,
    semantic_engine: Arc<Mutex<SemanticEngine>>,
    processed_transactions: usize,
    successful_transactions: usize,
    error_count: usize,
    message_queue: Vec<NetworkMessage>,
    contracts: Vec<SmartContract>,
    shard_events: Vec<ShardEvent>,
    errors: Vec<ErrorContext>,
    semantic_actions: Vec<SemanticAction>,
}

impl NetworkNode {
    pub fn new(
        config: NodeConfig,
        consensus: Arc<Mutex<ConsensusNode>>,
        error_analyzer: Arc<Mutex<ErrorAnalyzer>>,
        semantic_engine: Arc<Mutex<SemanticEngine>>,
    ) -> Self {
        Self {
            config,
            consensus,
            error_analyzer,
            semantic_engine,
            processed_transactions: 0,
            successful_transactions: 0,
            error_count: 0,
            message_queue: Vec::new(),
            contracts: Vec::new(),
            shard_events: Vec::new(),
            errors: Vec::new(),
            semantic_actions: Vec::new(),
        }
    }

    pub fn send_message(&mut self, message: NetworkMessage) {
        self.message_queue.push(message);
    }

    pub fn start_processing(&mut self) {
        while let Some(message) = self.message_queue.pop() {
            match message.message_type {
                MessageType::Transaction(transaction) => {
                    self.process_transaction(transaction);
                }
                MessageType::Consensus(consensus_message) => {
                    self.process_consensus_message(consensus_message);
                }
                MessageType::Error(error) => {
                    self.handle_error(error);
                }
                MessageType::SmartContract(contract) => {
                    self.process_smart_contract(contract);
                }
                MessageType::ShardEvent(event) => {
                    self.process_shard_event(event);
                }
                MessageType::ErrorEvent(error) => {
                    self.process_error_event(error);
                }
                MessageType::SemanticAction(action) => {
                    self.process_semantic_action(action);
                }
            }
        }
    }

    pub fn process_message(&mut self, message: NetworkMessage) {
        let entity_json = match &message.message_type {
            MessageType::Transaction(tx) => serde_json::to_value(tx).ok(),
            MessageType::SmartContract(sc) => serde_json::to_value(sc).ok(),
            MessageType::ShardEvent(ev) => serde_json::to_value(ev).ok(),
            _ => None,
        };
        if let Some(json) = entity_json {
            // Core consensus
            // You can log/save the result
            if let Ok(mut consensus) = self.consensus.lock() {
                let consensus_result = consensus.validate_entity(&json);
                // Можно логировать/сохранять результат
            }
            // Error analysis
            if let Ok(mut analyzer) = self.error_analyzer.lock() {
                let error = crate::error_analysis::semantic::analyze_any_error(&json);
                self.errors.push(error);
            }
            // Semantic analysis
            if let Ok(mut engine) = self.semantic_engine.lock() {
                let action = crate::semantic::SemanticAction {
                    entity_id: json.get("id").map(|v| v.to_string()).unwrap_or_default(),
                    action_type: "analyze".to_string(),
                    details: Some(format!("Semantic analysis for entity: {:?}", json)),
                };
                self.semantic_actions.push(action);
            }
        }
        match message.message_type {
            MessageType::Transaction(mut transaction) => {
                // Analyze interference through consensus
                if let Ok(mut consensus) = self.consensus.lock() {
                    let state_id = transaction.id.to_string();
                    // Add state to the node's field
                    let consensus_message = ConsensusMessage {
                        sender_id: message.sender.clone(),
                        state_id: transaction.id.to_string(),
                        raw_data: Bytes::from(transaction.data.clone()),
                        signature: vec![], // Assuming transaction doesn't have a signature field directly here
                        public_key: None, // TODO: Extract from msg or network registry
                    };
                    let pk = consensus.public_key.clone();
                    let _ = consensus.process_message(consensus_message, &pk);
                    // Check interference
                    let interference_result = consensus.check_interference(&state_id).unwrap_or(false);
                    if interference_result {
                        transaction.update_status(crate::transaction::TransactionStatus::Committed);
                    } else {
                        transaction.update_status(crate::transaction::TransactionStatus::Aborted);
                    }
                }
                self.processed_transactions += 1;
                // Save transaction (can be added to the node's transaction list)
                // self.transactions.push(transaction); // if there is a transactions field
            }
            MessageType::SmartContract(contract) => {
                self.process_smart_contract(contract);
            }
            MessageType::Consensus(consensus_message) => {
                self.process_consensus_message(consensus_message);
            }
            MessageType::Error(error) => {
                self.handle_error(error);
            }
            MessageType::ShardEvent(event) => {
                self.process_shard_event(event);
            }
            MessageType::ErrorEvent(error) => {
                self.process_error_event(error);
            }
            MessageType::SemanticAction(action) => {
                self.process_semantic_action(action);
            }
        }
    }

    fn process_transaction(&mut self, transaction: Transaction) {
        // Process transaction
        self.processed_transactions += 1;

        // Check transaction through the semantic engine
        if let Ok(mut semantic) = self.semantic_engine.lock() {
            if let Some(action) = semantic.analyze_transaction(&transaction) {
                // Apply action
                self.apply_semantic_action(action);
            }
        }

        // Check transaction through consensus
        if let Ok(mut consensus) = self.consensus.lock() {
            if consensus.validate_transaction(&transaction) {
                self.successful_transactions += 1;
            } else {
                self.error_count += 1;
            }
        }
    }

    fn process_consensus_message(&mut self, message: ConsensusMessage) {
        if let Ok(mut consensus) = self.consensus.lock() {
            let pk = consensus.public_key.clone();
            let _ = consensus.process_message(message, &pk);
        }
    }

    fn handle_error(&mut self, error: String) {
        self.error_count += 1;
        
        if let Ok(mut error_analyzer) = self.error_analyzer.lock() {
            error_analyzer.analyze_error(&error);
        }
    }

    fn apply_semantic_action(&mut self, action: crate::semantic::SemanticAction) {
        match action.action_type {
            crate::semantic::ActionType::TransactionAction { action, .. } => {
                // Apply action to transaction
                println!("Applying action to transaction: {}", action);
            }
            crate::semantic::ActionType::CoreAction { operation, .. } => {
                // Apply Core action
                println!("Applying Core action: {}", operation);
            }
            crate::semantic::ActionType::SystemAction { action, .. } => {
                // Apply system action
                println!("Applying system action: {}", action);
            }
        }
    }

    fn process_smart_contract(&mut self, mut contract: SmartContract) {
        // Core deployment through processor
        let processor = CoreSmartContractProcessor;
        if let Err(e) = processor.deploy(&mut contract) {
            contract.status = ContractStatus::Failed;
            // You can log the error
        }
        // TODO: Core consensus (similar to transactions)
        // self.consensus.lock().unwrap().validate_contract(&contract);
        // Save contract
        self.contracts.push(contract);
    }

    fn process_shard_event(&mut self, event: ShardEvent) {
        self.shard_events.push(event);
        // TODO: integration with shard logic, consensus, error/semantic
    }

    fn process_error_event(&mut self, error: ErrorContext) {
        self.errors.push(error);
        // TODO: integration with error_analyzer, logging, reaction
    }

    fn process_semantic_action(&mut self, action: SemanticAction) {
        self.semantic_actions.push(action);
        // TODO: integration with semantic_engine, applying actions
    }

    // Methods for obtaining statistics
    pub fn get_processed_transactions(&self) -> usize {
        self.processed_transactions
    }

    pub fn get_successful_transactions(&self) -> usize {
        self.successful_transactions
    }

    pub fn get_error_count(&self) -> usize {
        self.error_count
    }

    pub fn get_consensus_state(&self) -> Option<ConsensusState> {
        if let Ok(consensus) = self.consensus.lock() {
            Some(consensus.get_state())
        } else {
            None
        }
    }

    pub fn get_semantic_engine(&self) -> Option<Arc<Mutex<SemanticEngine>>> {
        Some(Arc::clone(&self.semantic_engine))
    }
} 


