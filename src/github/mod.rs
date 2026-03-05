use reqwest::Client;
use serde::{Deserialize, Serialize};

use log::{info, error, warn};
use std::env;

/// GitHub Codespaces API интеграция
pub struct GitHubCodespaces {
    client: Client,
    token: String,
    owner: String,
    repo: String,
}

#[derive(Debug, Deserialize)]
pub struct Machine {
    pub name: String,
    pub display_name: String,
    pub operating_system: String,
    pub storage_in_bytes: u64,
    pub memory_in_bytes: u64,
    pub cpus: u32,
}

#[derive(Debug, Deserialize)]
pub struct Codespace {
    pub id: u64, // GitHub API возвращает integer
    pub name: String,
    pub machine: Machine, // GitHub API возвращает объект
    pub state: String,
    pub url: String,
    pub web_url: String,
    pub location: String,
    pub idle_timeout_minutes: u32,
    pub retention_period_minutes: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct CreateCodespaceRequest {
    pub repository_id: u64,
    pub machine: Option<String>,
    pub location: Option<String>,
    pub idle_timeout_minutes: Option<u32>,
    pub retention_period_minutes: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCodespaceResponse {
    pub codespace: Codespace,
}

// GitHub API возвращает codespace напрямую, а не в поле codespace
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum CreateCodespaceResponseEnum {
    Wrapped { codespace: Codespace },
    Direct(Codespace),
}

#[derive(Debug, Deserialize)]
pub struct ListCodespacesResponse {
    pub codespaces: Vec<Codespace>,
    pub total_count: u32,
}

impl GitHubCodespaces {
    pub fn new(token: String, owner: String, repo: String) -> Self {
        Self {
            client: Client::new(),
            token,
            owner,
            repo,
        }
    }

    /// Создает новый GitHub Codespace
    pub async fn create_codespace(&self, machine: &str, location: &str) -> Result<Codespace, String> {
        let url = "https://api.github.com/user/codespaces";
        
        let request = CreateCodespaceRequest {
            repository_id: 987166369, // TRIAD repository ID
            machine: Some(machine.to_string()),
            location: Some(location.to_string()),
            idle_timeout_minutes: Some(240), // 4 часа (максимум)
            retention_period_minutes: Some(1440), // 24 часа
        };

        let response = self.client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "TRIAD-Network")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Failed to create codespace: {}", e))?;

        let status = response.status();
        if status.is_success() {
            let codespace_response: CreateCodespaceResponseEnum = response.json().await
                .map_err(|e| format!("Failed to parse response: {}", e))?;
            
            let codespace = match codespace_response {
                CreateCodespaceResponseEnum::Wrapped { codespace } => codespace,
                CreateCodespaceResponseEnum::Direct(codespace) => codespace,
            };
            
            info!("✅ Created GitHub Codespace: {} ({})", 
                  codespace.name, 
                  codespace.id);
            
            Ok(codespace)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(format!("Failed to create codespace: {} - {}", status, error_text))
        }
    }

    /// Получает список всех Codespaces
    pub async fn list_codespaces(&self) -> Result<Vec<Codespace>, String> {
        let url = "https://api.github.com/user/codespaces";
        
        let response = self.client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "TRIAD-Network")
            .send()
            .await
            .map_err(|e| format!("Failed to list codespaces: {}", e))?;

        let status = response.status();
        if status.is_success() {
            let response_data: ListCodespacesResponse = response.json().await
                .map_err(|e| format!("Failed to parse response: {}", e))?;
            
            info!("📋 Found {} GitHub Codespaces", response_data.total_count);
            Ok(response_data.codespaces)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(format!("Failed to list codespaces: {} - {}", status, error_text))
        }
    }

    /// Запускает существующий Codespace
    pub async fn start_codespace(&self, codespace_id: &str) -> Result<(), String> {
        let url = format!("https://api.github.com/user/codespaces/{}/start", codespace_id);
        
        let response = self.client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "TRIAD-Network")
            .send()
            .await
            .map_err(|e| format!("Failed to start codespace: {}", e))?;

        let status = response.status();
        if status.is_success() {
            info!("✅ Started GitHub Codespace: {}", codespace_id);
            Ok(())
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(format!("Failed to start codespace: {} - {}", status, error_text))
        }
    }

    /// Останавливает Codespace
    pub async fn stop_codespace(&self, codespace_id: &str) -> Result<(), String> {
        let url = format!("https://api.github.com/user/codespaces/{}/stop", codespace_id);
        
        let response = self.client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "TRIAD-Network")
            .send()
            .await
            .map_err(|e| format!("Failed to stop codespace: {}", e))?;

        let status = response.status();
        if status.is_success() {
            info!("🛑 Stopped GitHub Codespace: {}", codespace_id);
            Ok(())
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(format!("Failed to stop codespace: {} - {}", status, error_text))
        }
    }

    /// Удаляет Codespace
    pub async fn delete_codespace(&self, codespace_id: &str) -> Result<(), String> {
        let url = format!("https://api.github.com/user/codespaces/{}", codespace_id);
        
        let response = self.client
            .delete(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "TRIAD-Network")
            .send()
            .await
            .map_err(|e| format!("Failed to delete codespace: {}", e))?;

        let status = response.status();
        if status.is_success() {
            info!("🗑️  Deleted GitHub Codespace: {}", codespace_id);
            Ok(())
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(format!("Failed to delete codespace: {} - {}", status, error_text))
        }
    }

    /// Получает статус Codespace
    pub async fn get_codespace_status(&self, codespace_id: &str) -> Result<Codespace, String> {
        let url = format!("https://api.github.com/user/codespaces/{}", codespace_id);
        
        let response = self.client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "TRIAD-Network")
            .send()
            .await
            .map_err(|e| format!("Failed to get codespace status: {}", e))?;

        let status = response.status();
        if status.is_success() {
            let codespace: Codespace = response.json().await
                .map_err(|e| format!("Failed to parse response: {}", e))?;
            
            Ok(codespace)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(format!("Failed to get codespace status: {} - {}", status, error_text))
        }
    }

    /// Создает сеть из 10 Codespaces для TRIAD
    pub async fn create_triad_network(&self) -> Result<Vec<Codespace>, String> {
        info!("🚀 Creating TRIAD network with 10 GitHub Codespaces...");
        
        let mut codespaces = Vec::new();
        let machines = ["basicLinux32gb", "basicLinux32gb", "basicLinux32gb", 
                       "basicLinux32gb", "basicLinux32gb", "basicLinux32gb",
                       "basicLinux32gb", "basicLinux32gb", "basicLinux32gb", "basicLinux32gb"];
        let locations = ["WestUs2", "EastUs", "WestEurope", "SoutheastAsia", "AustraliaEast",
                        "BrazilSouth", "JapanEast", "SouthAfricaNorth", "CentralIndia", "NorthEurope"];

        for i in 0..10 {
            let machine = machines[i];
            let location = locations[i];
            
            info!("🌐 Creating Codespace {}/10: {} in {}", i + 1, machine, location);
            
            match self.create_codespace(machine, location).await {
                Ok(codespace) => {
                    let name = codespace.name.clone();
                    codespaces.push(codespace);
                    info!("✅ Codespace {}/10 created: {}", i + 1, name);
                },
                Err(e) => {
                    error!("❌ Failed to create Codespace {}/10: {}", i + 1, e);
                    // Продолжаем создание остальных
                }
            }
            
            // Небольшая задержка между запросами
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }

        info!("🎉 Created {} GitHub Codespaces for TRIAD network", codespaces.len());
        Ok(codespaces)
    }

    /// Останавливает всю сеть TRIAD
    pub async fn stop_triad_network(&self) -> Result<(), String> {
        info!("🛑 Stopping TRIAD network...");
        
        let codespaces = self.list_codespaces().await?;
        let triad_codespaces: Vec<_> = codespaces
            .into_iter()
            .filter(|c| c.name.starts_with("triad-node-"))
            .collect();

        info!("🛑 Found {} TRIAD codespaces to stop", triad_codespaces.len());

        for codespace in triad_codespaces {
            if let Err(e) = self.stop_codespace(&codespace.id.to_string()).await {
                warn!("⚠️  Failed to stop codespace {}: {}", codespace.name, e);
            }
        }

        info!("✅ TRIAD network stopped");
        Ok(())
    }
}

/// Менеджер GitHub Codespaces для распределенной сети
pub struct GitHubCodespaceManager {
    client: Client,
    token: String,
    repo_id: u64,
}

impl GitHubCodespaceManager {
    pub fn new() -> Self {
        // Try env var first
        if let Ok(token) = env::var("GITHUB_TOKEN") {
            return Self { client: Client::new(), token, repo_id: 987166369 };
        }
        // Try GitHub CLI: gh auth token
        if let Ok(output) = std::process::Command::new("gh").args(["auth", "token"]).output() {
            if output.status.success() {
                if let Ok(mut token) = String::from_utf8(output.stdout) {
                    token = token.trim().to_string();
                    if !token.is_empty() {
                        info!("Imported GitHub token via gh auth token");
                        return Self { client: Client::new(), token, repo_id: 987166369 };
                    }
                }
            } else {
                warn!("gh auth token failed; falling back to simulation");
            }
        } else {
            warn!("gh CLI not found; falling back to simulation");
        }
        // Fallback: simulation
        warn!("GITHUB_TOKEN not set; using simulation mode for Codespaces (local nodes will be used)");
        Self::new_simulation()
    }

    pub fn new_simulation() -> Self {
        Self {
            client: Client::new(),
            token: "simulation-token".to_string(),
            repo_id: 0,
        }
    }

    /// Lists Codespaces (id, name). Returns empty in simulation mode
    pub async fn list_codespaces(&self) -> Result<Vec<(String, String)>, String> {
        if self.repo_id == 0 || self.token == "simulation-token" {
            return Ok(Vec::new());
        }
        let url = "https://api.github.com/user/codespaces";
        let response = self.client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "TRIAD-Network")
            .send()
            .await
            .map_err(|e| format!("Failed to list codespaces: {}", e))?;

        if response.status().is_success() {
            let json: serde_json::Value = response.json().await
                .map_err(|e| format!("Failed to parse list response: {}", e))?;
            let mut out = Vec::new();
            if let Some(arr) = json["codespaces"].as_array() {
                for cs in arr {
                    let name = cs["name"].as_str().unwrap_or("").to_string();
                    let id = cs["id"].as_str()
                        .map(|s| s.to_string())
                        .or_else(|| cs["id"].as_u64().map(|u| u.to_string()))
                        .unwrap_or_default();
                    if !name.is_empty() && !id.is_empty() {
                        out.push((id, name));
                    }
                }
            }
            Ok(out)
        } else {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            Err(format!("Failed to list codespaces: {} - {}", status, error_text))
        }
    }

    /// Создает новый GitHub Codespace
    pub async fn create_codespace(&self, node_id: &str, port: u16, shard_id: usize) -> Result<String, String> {
        if self.repo_id == 0 || self.token == "simulation-token" {
            return Err("GitHub token not available - cannot create codespace".to_string());
        }
        
        let url = "https://api.github.com/user/codespaces";
        
        let request = serde_json::json!({
            "repository_id": self.repo_id,
            "machine": "basicLinux32gb",
            "location": "WestUs2",
            "idle_timeout_minutes": 30,
            "display_name": node_id,
            "retention_period_minutes": 480,
            "ref": "main"
        });

        let response = self.client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "TRIAD-Network")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Failed to create codespace: {}", e))?;

        let status = response.status();
        if status.is_success() {
            let response_json: serde_json::Value = response.json().await
                .map_err(|e| format!("Failed to parse response: {}", e))?;
            
            // Добавляем отладочную информацию
            println!("DEBUG: Full response: {}", serde_json::to_string_pretty(&response_json).unwrap());
            
            let codespace_id = response_json["id"]
                .as_u64()
                .ok_or_else(|| {
                    let response_str = serde_json::to_string_pretty(&response_json).unwrap();
                    format!("No codespace ID in response. Full response: {}", response_str)
                })?
                .to_string();
            
            info!("✅ Created GitHub Codespace: {} ({})", node_id, codespace_id);
            Ok(codespace_id)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(format!("Failed to create codespace: {} - {}", status, error_text))
        }
    }

    /// Запускает существующий Codespace
    pub async fn start_codespace(&self, codespace_id: &str) -> Result<(), String> {
        if self.repo_id == 0 || self.token == "simulation-token" {
            return Err("GitHub token not available - cannot start codespace".to_string());
        }
        
        let url = format!("https://api.github.com/user/codespaces/{}/start", codespace_id);
        
        let response = self.client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "TRIAD-Network")
            .send()
            .await
            .map_err(|e| format!("Failed to start codespace: {}", e))?;

        let status = response.status();
        if status.is_success() {
            info!("✅ Started GitHub Codespace: {}", codespace_id);
            Ok(())
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(format!("Failed to start codespace: {} - {}", status, error_text))
        }
    }

    /// Получает статус Codespace
    pub async fn get_codespace_status(&self, codespace_id: &str) -> Result<String, String> {
        let url = format!("https://api.github.com/user/codespaces/{}", codespace_id);
        
        let response = self.client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "TRIAD-Network")
            .send()
            .await
            .map_err(|e| format!("Failed to get codespace status: {}", e))?;

        let status = response.status();
        if status.is_success() {
            let response_json: serde_json::Value = response.json().await
                .map_err(|e| format!("Failed to parse response: {}", e))?;
            
            let state = response_json["state"]
                .as_str()
                .ok_or("No state in response")?
                .to_string();
            
            Ok(state)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(format!("Failed to get codespace status: {} - {}", status, error_text))
        }
    }

    /// Выполняет команду в Codespace
    pub async fn run_command_in_codespace(&self, codespace_id: &str, command: &str) -> Result<(), String> {
        // Используем GitHub CLI для выполнения команд в codespace
        let output = tokio::process::Command::new("gh")
            .args(&["codespace", "exec", "--codespace", codespace_id, "--command", command])
            .output()
            .await
            .map_err(|e| format!("Failed to execute command in codespace: {}", e))?;

        if output.status.success() {
            info!("✅ Command executed successfully in codespace {}", codespace_id);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Command failed in codespace {}: {}", codespace_id, stderr))
        }
    }

    /// Останавливает Codespace
    pub async fn stop_codespace(&self, codespace_id: &str) -> Result<(), String> {
        let url = format!("https://api.github.com/user/codespaces/{}/stop", codespace_id);
        
        let response = self.client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "TRIAD-Network")
            .send()
            .await
            .map_err(|e| format!("Failed to stop codespace: {}", e))?;

        let status = response.status();
        if status.is_success() {
            info!("🛑 Stopped GitHub Codespace: {}", codespace_id);
            Ok(())
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(format!("Failed to stop codespace: {} - {}", status, error_text))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_github_codespaces_new() {
        let github = GitHubCodespaces::new(
            "test_token".to_string(),
            "test_owner".to_string(),
            "test_repo".to_string(),
        );
        
        assert_eq!(github.token, "test_token");
        assert_eq!(github.owner, "test_owner");
        assert_eq!(github.repo, "test_repo");
    }
}


