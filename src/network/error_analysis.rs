use std::collections::HashMap;
use std::time::{SystemTime, Duration};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorCategory {
    Connection,
    Authentication,
    Protocol,
    Resource,
    Configuration,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    pub timestamp: SystemTime,
    pub node_id: Option<Uuid>,
    pub peer_id: Option<Uuid>,
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub description: String,
    pub stack_trace: Option<String>,
    pub system_state: HashMap<String, String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorAnalyzer {
    errors: HashMap<Uuid, ErrorAnalysis>,
    error_patterns: HashMap<String, Vec<String>>,
    solutions_history: HashMap<String, Vec<String>>,
}

impl ErrorAnalyzer {
    pub fn new() -> Self {
        Self {
            errors: HashMap::new(),
            error_patterns: HashMap::new(),
            solutions_history: HashMap::new(),
        }
    }

    pub fn analyze_error(&mut self, context: ErrorContext) -> ErrorAnalysis {
        let error_id = Uuid::new_v4();
        
        // Root cause analysis
        let root_cause = self.determine_root_cause(&context);
        
        // Search for possible solutions
        let possible_solutions = self.find_possible_solutions(&context, &root_cause);
        
        // Check for repeated errors
        let recurrence_count = self.check_error_recurrence(&context);
        
        // Search for related errors
        let related_errors = self.find_related_errors(&context);

        let analysis = ErrorAnalysis {
            error_id,
            context,
            root_cause,
            possible_solutions,
            applied_solution: None,
            resolution_time: None,
            recurrence_count,
            related_errors,
        };

        self.errors.insert(error_id, analysis.clone());
        analysis
    }

    fn determine_root_cause(&self, context: &ErrorContext) -> String {
        match context.category {
            ErrorCategory::Connection => {
                if context.description.contains("Address already in use") {
                    "Port is already used by another process".to_string()
                } else if context.description.contains("Connection refused") {
                    "Remote node is unavailable or refuses connection".to_string()
                } else {
                    "Unknown connection problem".to_string()
                }
            },
            ErrorCategory::Authentication => {
                "Node authentication problem".to_string()
            },
            ErrorCategory::Protocol => {
                "Protocol or version mismatch".to_string()
            },
            ErrorCategory::Resource => {
                "Insufficient system resources".to_string()
            },
            ErrorCategory::Configuration => {
                "Incorrect network configuration".to_string()
            },
            ErrorCategory::Unknown => {
                "Unknown error cause".to_string()
            },
        }
    }

    fn find_possible_solutions(&self, context: &ErrorContext, root_cause: &str) -> Vec<String> {
        let mut solutions = Vec::new();
        
        match context.category {
            ErrorCategory::Connection => {
                if root_cause.contains("port is already used") {
                    solutions.push("Free the port by terminating the process using it".to_string());
                    solutions.push("Change the port in the node configuration".to_string());
                    solutions.push("Add automatic port search".to_string());
                }
            },
            ErrorCategory::Authentication => {
                solutions.push("Check and update authentication keys".to_string());
                solutions.push("Reconnect with new credentials".to_string());
            },
            ErrorCategory::Protocol => {
                solutions.push("Update protocol version".to_string());
                solutions.push("Add backward compatibility support".to_string());
            },
            ErrorCategory::Resource => {
                solutions.push("Increase system resource limits".to_string());
                solutions.push("Optimize resource usage".to_string());
            },
            ErrorCategory::Configuration => {
                solutions.push("Check and fix configuration files".to_string());
                solutions.push("Reset settings to default values".to_string());
            },
            ErrorCategory::Unknown => {
                solutions.push("Collect additional diagnostic information".to_string());
                solutions.push("Check system logs".to_string());
            },
        }

        solutions
    }

    fn check_error_recurrence(&self, context: &ErrorContext) -> u32 {
        // Count similar errors in history
        self.errors.values()
            .filter(|e| e.context.category == context.category && 
                        e.context.description == context.description)
            .count() as u32
    }

    fn find_related_errors(&self, context: &ErrorContext) -> Vec<Uuid> {
        // Search for related errors by category and time
        self.errors.values()
            .filter(|e| e.context.category == context.category &&
                        e.context.timestamp > SystemTime::now() - Duration::from_secs(3600))
            .map(|e| e.error_id)
            .collect()
    }

    pub fn apply_solution(&mut self, error_id: Uuid, solution: String) -> Result<(), String> {
        if let Some(error) = self.errors.get_mut(&error_id) {
            error.applied_solution = Some(solution.clone());
            error.resolution_time = Some(SystemTime::now()
                .duration_since(error.context.timestamp)
                .unwrap_or(Duration::from_secs(0)));
            
            // Save solution to history
            self.solutions_history
                .entry(error.context.category.to_string())
                .or_insert_with(Vec::new)
                .push(solution);
            
            Ok(())
        } else {
            Err("Error not found".to_string())
        }
    }

    pub fn get_error_statistics(&self) -> HashMap<String, u32> {
        let mut stats = HashMap::new();
        
        for error in self.errors.values() {
            let category = format!("{:?}", error.context.category);
            *stats.entry(category).or_insert(0) += 1;
        }
        
        stats
    }

    pub fn get_successful_solutions(&self) -> HashMap<String, Vec<String>> {
        self.solutions_history.clone()
    }
} 

