use serde::{Serialize, Deserialize};
use rand::Rng;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use std::str::FromStr;

/// Типы полезной нагрузки транзакций, имитирующие реальный трафик Solana
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionPayload {
    /// Jupiter V6 Swap (40%) - Сложный маршрут обмена
    JupiterSwap {
        in_token: String,
        out_token: String,
        amount_in: u64,
        min_out: u64,
        route_steps: Vec<String>, // Список AMM пулов в маршруте
        platform_fee_bps: u8,
        route_complexity: u8, // New field for CPI simulation
    },
    
    /// Drift Perpetual (25%) - Открытие/закрытие позиций
    DriftPerpOrder {
        market_index: u16,
        order_type: String, // "Limit", "Market", "Trigger"
        direction: String,  // "Long", "Short"
        base_asset_amount: u64,
        price: u64,
        reduce_only: bool,
        matching_complexity: u8, // New field
    },
    
    /// Phoenix Spot (15%) - Лимитные ордера в книге заявок
    PhoenixLimitOrder {
        market: String,
        side: String, // "Bid", "Ask"
        price_lots: u64,
        size_lots: u64,
        client_order_id: u128,
        time_in_force: String, // "IOC", "GTC", "FOK"
    },
    
    /// Serum Legacy (10%) - Для совместимости
    SerumDexInstruction {
        program_id: String,
        instruction: Vec<u8>,
        accounts: Vec<String>,
    },
    
    /// Token Transfer (8%) - Простые переводы и CPI
    TokenTransfer {
        mint: String,
        destination: String,
        amount: u64,
        decimals: u8,
    },
    
    /// NFT Mint/Burn (2%) - Metaplex операции
    NftOperation {
        collection: String,
        operation: String, // "Mint", "Burn", "UpdateMetadata"
        metadata_uri: String,
    },

    /// Native SVM Instruction (TRVM Kernel) - Phase 2 Integration
    SvmInstruction {
        program_id: String,
        accounts: Vec<String>,
        data: Vec<u8>,
    },
}

impl TransactionPayload {
    /// Генерирует случайную полезную нагрузку в соответствии с заданным распределением
    pub fn generate_random() -> Self {
        let mut rng = rand::thread_rng();
        let roll = rng.gen_range(0..100);
        
        if roll < 40 {
            // 40% Jupiter Swap
            Self::JupiterSwap {
                in_token: "So11111111111111111111111111111111111111112".to_string(),
                out_token: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                amount_in: rng.gen_range(1_000_000..100_000_000_000),
                min_out: rng.gen_range(1_000_000..100_000_000_000),
                route_steps: vec!["Raydium".to_string(), "Orca".to_string(), "Meteora".to_string()],
                platform_fee_bps: rng.gen_range(0..50),
                // Simulate complex route (8-14 CPI)
                route_complexity: rng.gen_range(8..15),
            }
        } else if roll < 65 {
            // 25% Drift Perp (40 + 25 = 65)
            Self::DriftPerpOrder {
                market_index: rng.gen_range(0..10),
                order_type: if rng.gen_bool(0.7) { "Limit".to_string() } else { "Market".to_string() },
                direction: if rng.gen_bool(0.5) { "Long".to_string() } else { "Short".to_string() },
                base_asset_amount: rng.gen_range(100..10_000),
                price: rng.gen_range(10_000..500_000),
                reduce_only: rng.gen_bool(0.3),
                // Simulate order matching complexity
                matching_complexity: rng.gen_range(1..5),
            }
        } else if roll < 80 {
            // 15% Phoenix Spot (65 + 15 = 80)
            Self::PhoenixLimitOrder {
                market: "SOL/USDC".to_string(),
                side: if rng.gen_bool(0.5) { "Bid".to_string() } else { "Ask".to_string() },
                price_lots: rng.gen_range(1000..2000),
                size_lots: rng.gen_range(1..100),
                client_order_id: rng.r#gen(),
                time_in_force: "GTC".to_string(),
            }
        } else if roll < 90 {
            // 10% Serum Legacy (80 + 10 = 90)
            Self::SerumDexInstruction {
                program_id: "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin".to_string(),
                instruction: vec![1, 2, 3, 4, 5], // Placeholder
                accounts: vec!["Account1".to_string(), "Account2".to_string()],
            }
        } else if roll < 98 {
            // 8% Token Transfer (90 + 8 = 98)
            Self::TokenTransfer {
                mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
                destination: "DestAccount123".to_string(),
                amount: rng.gen_range(100..1_000_000),
                decimals: 6,
            }
        } else {
            // 2% NFT Operation (98 + 2 = 100)
            Self::NftOperation {
                collection: "MadLads".to_string(),
                operation: if rng.gen_bool(0.8) { "UpdateMetadata".to_string() } else { "Mint".to_string() },
                metadata_uri: "https://arweave.net/metadata.json".to_string(),
            }
        }
    }
    
    /// Сериализует payload в байты для включения в транзакцию
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(self).unwrap_or_default()
    }

    /// Преобразует payload в инструкцию SVM (если применимо)
    pub fn to_svm_instruction(&self) -> Option<Instruction> {
        match self {
            Self::SvmInstruction { program_id, accounts, data } => {
                let program_id = Pubkey::from_str(program_id).ok()?;
                let accounts = accounts.iter().map(|acc| {
                    AccountMeta {
                        pubkey: Pubkey::from_str(acc).unwrap_or_default(),
                        is_signer: false, // Simplification for now
                        is_writable: true,
                    }
                }).collect();
                Some(Instruction {
                    program_id,
                    accounts,
                    data: data.clone(),
                })
            },
            Self::TokenTransfer { mint, destination, amount, .. } => {
                // Example of "hollowing" simple transfers into SVM instructions
                // (This is a simplified mock of spl-token transfer)
                let program_id = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
                let mint_pubkey = Pubkey::from_str(mint).ok()?;
                let dest_pubkey = Pubkey::from_str(destination).ok()?;
                
                let mut data = vec![3]; // Transfer instruction index
                data.extend_from_slice(&amount.to_le_bytes());
                
                Some(Instruction {
                    program_id,
                    accounts: vec![
                        AccountMeta::new(mint_pubkey, false),
                        AccountMeta::new(dest_pubkey, false),
                    ],
                    data,
                })
            },
            _ => None,
        }
    }
}


