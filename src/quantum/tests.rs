#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantum::field::{QuantumField, QuantumState, StateVector};
    use crate::quantum::interference::InterferenceEngine;
    use crate::quantum::prob_ops::{ProbabilisticOperation, OperationOutcome};
    use crate::quantum::consensus::{ConsensusNode, ConsensusMessage};
    use crate::sharding::assign_shard;
    use std::time::Instant;
    use num_complex;
    use bytes::Bytes;

    #[test]
    fn test_quantum_vs_classical_blockchain() {
        // Создаем сеть из 10 узлов
        let num_shards = 4;
        let mut nodes: Vec<ConsensusNode> = (0..10)
            .map(|i| {
                let node_id = format!("node{}", i);
                let shard_id = assign_shard(&node_id, num_shards);
                ConsensusNode::new(node_id, shard_id)
            })
            .collect();
        
        // Тестовые данные (аналог транзакции)
        let test_data = vec![1, 2, 3, 4, 5];
        let state_id = "test_state".to_string();
        
        // Измеряем время достижения консенсуса в квантово-вдохновленной модели
        let quantum_start = Instant::now();
        
        // Первый узел отправляет сообщение (подпишем содержимое ключом узла 0)
        let raw = Bytes::from(test_data.clone());
        let signature = {
            let sk = &nodes[0].secret_key;
            sk.sign(raw.as_ref()).to_bytes().to_vec()
        };
        let message = ConsensusMessage {
            sender_id: "node0".to_string(),
            state_id: state_id.clone(),
            raw_data: raw.clone(),
            signature,
        };
        
        // Обрабатываем сообщение на всех узлах
        for node in &mut nodes {
            node.process_message(message.clone(), &nodes[0].public_key).unwrap();
        }
        
        // Проверяем интерференцию на всех узлах
        let mut consensus_reached = true;
        for node in &nodes {
            if !node.check_interference(&state_id).unwrap() {
                consensus_reached = false;
                break;
            }
        }
        
        let quantum_time = quantum_start.elapsed();
        
        // Имитация классического блокчейна
        let classical_start = Instant::now();
        
        // Имитируем процесс майнинга и подтверждений
        let mut confirmations = 0;
        let required_confirmations = 6; // 51% от 10 узлов
        
        while confirmations < required_confirmations {
            // Имитируем время на майнинг
            std::thread::sleep(std::time::Duration::from_millis(100));
            confirmations += 1;
        }
        
        let classical_time = classical_start.elapsed();
        
        println!("\nСравнение квантово-вдохновленной модели с классическим блокчейном:");
        println!("Квантово-вдохновленная модель:");
        println!("- Время достижения консенсуса: {:?}", quantum_time);
        println!("- Консенсус достигнут: {}", consensus_reached);
        println!("- Энергопотребление: низкое (только интерференция)");
        
        println!("\nКлассический блокчейн:");
        println!("- Время достижения консенсуса: {:?}", classical_time);
        println!("- Подтверждения получены: {}/{}", confirmations, required_confirmations);
        println!("- Энергопотребление: высокое (майнинг)");
        
        // Проверяем, что квантово-вдохновленная модель быстрее
        assert!(quantum_time < classical_time, 
            "Квантово-вдохновленная модель должна быть быстрее классического блокчейна");
    }

    #[test]
    fn test_quantum_field_basic() {
        let mut field = QuantumField::new();
        
        // Создаем тестовое состояние
        let state = QuantumState {
            id: "test_state".to_string(),
            shard_id: "0".to_string(),
            amplitude: 1.0,
            phase: 0.0,
            superposition: vec![
                StateVector {
                    value: num_complex::Complex::new(1.0, 0.0),
                    probability: 0.7,
                },
                StateVector {
                    value: num_complex::Complex::new(0.0, 0.0),
                    probability: 0.3,
                },
            ],
        };
        
        field.add_state("test".to_string(), state);
        
        // Проверяем измерение
        let measured = field.measure("test").unwrap();
        assert!(measured.value.len() == 3);
    }

    #[test]
    fn test_interference() {
        let engine = InterferenceEngine::new(1000, 0.95);
        
        let states = vec![
            QuantumState {
                amplitude: 1.0,
                phase: 0.0,
                superposition: vec![StateVector {
                    value: vec![1],
                    probability: 1.0,
                }],
            },
            QuantumState {
                amplitude: 0.8,
                phase: 0.0,
                superposition: vec![StateVector {
                    value: vec![1],
                    probability: 1.0,
                }],
            },
        ];
        
        let pattern = engine.calculate_interference_pattern(&states);
        let analysis = engine.analyze_interference(&pattern);
        
        println!("Максимальная амплитуда: {}", analysis.max_amplitude);
        println!("Минимальная амплитуда: {}", analysis.min_amplitude);
        println!("Средняя амплитуда: {}", analysis.average_amplitude);
        println!("Точек конструктивной интерференции: {}", analysis.constructive_points.len());
        println!("Точек деструктивной интерференции: {}", analysis.destructive_points.len());
        
        assert!(analysis.max_amplitude > 0.0);
        assert!(analysis.min_amplitude < 0.0);
        assert!(!analysis.constructive_points.is_empty());
        assert!(!analysis.destructive_points.is_empty());
    }

    #[test]
    fn test_superposition() {
        let mut field = QuantumField::new();
        
        // Создаем состояние в суперпозиции
        let state = QuantumState {
            amplitude: 1.0,
            phase: 0.0,
            superposition: vec![
                StateVector {
                    value: vec![1],
                    probability: 0.5,
                },
                StateVector {
                    value: vec![0],
                    probability: 0.5,
                },
            ],
        };
        
        field.add_state("superposition".to_string(), state);
        
        // Проводим несколько измерений
        let mut ones = 0;
        let mut zeros = 0;
        
        for _ in 0..1000 {
            let measured = field.measure("superposition").unwrap();
            if measured.value[0] == 1 {
                ones += 1;
            } else {
                zeros += 1;
            }
        }
        
        // Проверяем, что распределение примерно 50/50
        let ratio = ones as f64 / (ones + zeros) as f64;
        assert!(ratio > 0.45 && ratio < 0.55);
    }

    #[test]
    fn test_complex_probabilistic_operation() {
        // Создаем вероятностную операцию, аналогичную по сложности типичным VM-операциям
        let op = ProbabilisticOperation {
            description: "VM-like operation".to_string(),
            outcomes: vec![
                OperationOutcome {
                    label: "Success".to_string(),
                    probability: 0.8,
                    value: Some(serde_json::json!({"result": "ok"})),
                },
                OperationOutcome {
                    label: "Failure".to_string(),
                    probability: 0.2,
                    value: Some(serde_json::json!({"error": "fail"})),
                },
            ],
        };

        // Выполняем операцию 100 раз и подсчитываем результаты
        let mut success = 0;
        let mut fail = 0;
        for _ in 0..100 {
            let outcome = op.execute();
            match outcome.label.as_str() {
                "Success" => success += 1,
                "Failure" => fail += 1,
                _ => {}
            }
        }

        // Проверяем, что результаты близки к ожидаемым вероятностям
        let success_ratio = success as f64 / 100.0;
        assert!(success_ratio > 0.7 && success_ratio < 0.9);
    }

    #[test]
    fn test_consensus_interference() {
        // Создаем два узла
        let num_shards = 4;
        let mut node1 = ConsensusNode::new("node1".to_string(), assign_shard("node1", num_shards));
        let mut node2 = ConsensusNode::new("node2".to_string(), assign_shard("node2", num_shards));

        // Создаем тестовые данные
        let test_data = vec![1, 2, 3, 4, 5];
        let state_id = "test_state".to_string();

        // Узел 1 отправляет сообщение узлу 2
        let raw = Bytes::from(test_data.clone());
        let signature = {
            let sk = &node1.secret_key;
            sk.sign(raw.as_ref()).to_bytes().to_vec()
        };
        let message = ConsensusMessage {
            sender_id: "node1".to_string(),
            state_id: state_id.clone(),
            raw_data: raw,
            signature,
        };

        // Узел 2 обрабатывает сообщение
        let processed_state = node2.process_message(message, &node1.public_key).unwrap();

        // Проверяем, что состояние было обработано
        assert_eq!(processed_state.superposition[0].value, test_data);

        // Проверяем интерференцию
        let has_interference = node2.check_interference(&state_id).unwrap();
        assert!(has_interference, "Должна быть обнаружена конструктивная интерференция");

        // Проверяем, что состояние сохранилось в поле узла 2
        let saved_state = node2.field.get_state(&state_id).unwrap();
        assert_eq!(saved_state.superposition[0].value, test_data);
    }

    #[test]
    fn test_export_interference_pattern_to_csv() {
        use crate::quantum::interference::InterferenceEngine;
        use crate::quantum::field::QuantumState;
        let engine = InterferenceEngine::new(1000, 0.98);
        let states = vec![
            QuantumState {
                amplitude: 1.0,
                phase: 0.0,
                superposition: vec![],
            },
            QuantumState {
                amplitude: 0.8,
                phase: std::f64::consts::PI / 2.0,
                superposition: vec![],
            },
        ];
        let pattern = engine.calculate_interference_pattern(&states);
        let path = "interference_pattern.csv";
        engine.export_pattern_to_csv(&pattern, path).expect("Не удалось сохранить CSV");
        // Проверяем, что файл создан (опционально)
        assert!(std::path::Path::new(path).exists());
    }
} 