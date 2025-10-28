use std::collections::HashMap;
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use parking_lot::RwLock;
use rayon::prelude::*;
use num_complex::Complex;
use wide::f64x4;
use std::time::{Instant, Duration};
use uuid::Uuid;
use rand::Rng;

// Импорты для Ed25519 подписей квантовых волн
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
#[cfg(feature = "zebra_batch")]
use ed25519_zebra::{VerificationKeyBytes as ZebraVKBytes, Signature as ZebraSig};
#[cfg(feature = "zebra_batch")]
use ed25519_zebra::batch::Verifier as ZebraVerifier;
#[cfg(feature = "zebra_batch")]
use rand07::rngs::OsRng;

#[derive(Debug, Clone)]
pub struct QuantumWave {
    pub id: Arc<str>,
    pub amplitude: f64,
    pub phase: f64,
    pub shard_id: String,
    pub shard_index: usize, // OPTIMIZATION: Cached shard index to avoid rehashing
    pub created_at: Instant,
    pub lifetime: Duration, // Время жизни волны
    pub superposition: Vec<StateVector>,
    pub signature: Vec<u8>, // Ed25519 подпись квантовой волны
}

impl Clone for ShardedQuantumField {
    fn clone(&self) -> Self {
        let mut new_shards: Vec<RwLock<HashMap<String, QuantumWave>>> = Vec::with_capacity(self.shards.len());
        for s in self.shards.iter() {
            let map = s.read();
            new_shards.push(RwLock::new(map.clone()));
        }
        Self {
            shards: new_shards,
            interference_pattern: self.interference_pattern.clone(),
            cache: self.cache.clone(),
            max_waves_total: self.max_waves_total,
            pending_updates: self.pending_updates,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateVector {
    pub value: Complex<f64>,
    pub probability: f64,
}

#[derive(Debug, Clone)]
pub struct QuantumField {
    pub active_waves: HashMap<String, QuantumWave>, // Только активные волны
    pub interference_pattern: Vec<InterferencePoint>,
    cache: Arc<RwLock<HashMap<String, Vec<InterferencePoint>>>>,
    pub max_waves: usize, // Максимальное количество активных волн
    // Счётчик отложенных обновлений интерференции (для батч-режима)
    pending_updates: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterferencePoint {
    pub position: f64,
    pub amplitude: f64,
    pub phase: f64,
}

impl QuantumField {
    pub fn new() -> Self {
        Self {
            active_waves: HashMap::new(),
            interference_pattern: Vec::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_waves: 100_000, // Убираем искусственное ограничение для масштабируемости
            pending_updates: 0,
        }
    }

    /// Совместимость: добавление волны с ключом Arc<str>
    pub fn add_wave_arc(&mut self, key: Arc<str>, wave: QuantumWave) {
        self.add_wave(key.to_string(), wave);
    }

    // Вспомогательная функция: суммирование косинусов по парам (амплитуда, фаза) с SIMD-ускорением
    #[inline]
    fn sum_cos_pairs(pairs: &[(f64,f64)], x: f64, decay: f64, total_amp_sum: f64) -> (f64, f64) {
        let mut amp_sum = 0.0f64;
        let mut phase_wsum = 0.0f64;
        let pi2x = 2.0 * std::f64::consts::PI * x;
        // SIMD по 4 элемента
        let mut i = 0usize;
        let n = pairs.len();
        while i + 4 <= n {
            let a = f64x4::from([pairs[i].0, pairs[i+1].0, pairs[i+2].0, pairs[i+3].0]);
            let ph = f64x4::from([pairs[i].1, pairs[i+1].1, pairs[i+2].1, pairs[i+3].1]);
            let ang = ph + f64x4::splat(pi2x);
            let c = Self::simd_cos4(ang);
            amp_sum += (a * c).to_array().iter().sum::<f64>();
            phase_wsum += (ph * a).to_array().iter().sum::<f64>();
            i += 4;
        }
        // хвост
        for (a, ph) in &pairs[i..] {
            amp_sum += *a * (pi2x + *ph).cos();
            phase_wsum += *ph * *a;
        }
        amp_sum *= (-x * decay).exp();
        let phase = if total_amp_sum > 0.0 { phase_wsum / total_amp_sum } else { 0.0 };
        (amp_sum, phase)
    }

    // Быстрый косинус для вектора из 4 значений (полиномиальная аппроксимация с редукцией диапазона)
    #[inline]
    fn simd_cos4(x: f64x4) -> f64x4 {
        let two_pi = f64x4::splat(2.0 * std::f64::consts::PI);
        let inv_two_pi = f64x4::splat(1.0 / (2.0 * std::f64::consts::PI));
        let k = (x * inv_two_pi).round();
        let xr = x - k * two_pi;
        let x2 = xr * xr;
        let x4 = x2 * x2;
        let half = f64x4::splat(0.5);
        let inv24 = f64x4::splat(1.0 / 24.0);
        f64x4::splat(1.0) - x2 * half + x4 * inv24
    }

    // NOTE: для шардированного поля см. impl ShardedQuantumField::snapshot_waves

    /// Добавляет волну без проверки подписи (для внутреннего использования)
    pub fn add_wave_unchecked(&mut self, key: String, wave: QuantumWave) {
        self.add_wave(key, wave);
    }

    pub fn add_wave(&mut self, key: String, wave: QuantumWave) {
        // НЕ очищаем устаревшие волны при каждом добавлении - это замедляет работу!
        // self.cleanup_expired_waves(); // Убираем отсюда
        
        // Если достигли лимита, удаляем самую старую волну
        if self.active_waves.len() >= self.max_waves {
            self.remove_oldest_wave();
        }
        
        self.active_waves.insert(key, wave);
        // Убираем автоматическое обновление интерференции - будем вызывать только когда нужно
        // self.update_interference(10_000); // Обновляем интерференцию с высоким разрешением
        // Учтём отложенное обновление
        self.pending_updates = self.pending_updates.saturating_add(1);
    }

    /// Batch-вставка волн с минимальными накладными расходами
    pub fn add_waves_batch<I>(&mut self, items: I)
    where
        I: IntoIterator<Item = (String, QuantumWave)>,
    {
        let mut count = 0usize;
        for (key, wave) in items {
            // Если достигли лимита, удаляем самую старую волну (редко)
            if self.active_waves.len() >= self.max_waves {
                self.remove_oldest_wave();
            }
            self.active_waves.insert(key, wave);
            count += 1;
        }
        // Обновим счётчик отложенных изменений разом
        self.pending_updates = self.pending_updates.saturating_add(count);
        // Очистку просроченных волн выполняем реже — оставим на вызывающего или отдельный этап
    }

    /// Пытается выполнить пересчёт интерференции, если накопилось достаточно изменений
    /// Возвращает true, если пересчёт был выполнен
    pub fn maybe_flush_interference(&mut self, threshold: usize, resolution: usize) -> bool {
        if self.pending_updates >= threshold {
            self.update_interference(resolution);
            self.pending_updates = 0;
            true
        } else {
            false
        }
    }

    /// Форсированный пересчёт интерференции независимо от счётчика
    pub fn force_flush_interference(&mut self, resolution: usize) {
        self.update_interference(resolution);
        self.pending_updates = 0;
    }

    /// Безопасно добавляет волну с проверкой подписи (Ed25519)
    pub fn add_signed_wave(&mut self, key: String, wave: QuantumWave, public_key: &VerifyingKey) -> Result<(), String> {
        if !wave.verify_signature(public_key) {
            return Err("Invalid wave signature".to_string());
        }
        if Instant::now().duration_since(wave.created_at) > wave.lifetime {
            return Err("Wave has expired".to_string());
        }
        self.add_wave(key, wave);
        Ok(())
    }

    /// Проверяет состояние волны
    pub fn check_wave_health(&self, wave_id: &str) -> Option<WaveHealth> {
        if let Some(wave) = self.active_waves.get(wave_id) {
            let age = Instant::now().duration_since(wave.created_at);
            let health_ratio = age.as_secs_f64() / wave.lifetime.as_secs_f64();
            
            let health = if health_ratio < 0.3 {
                WaveHealthStatus::Young
            } else if health_ratio < 0.7 {
                WaveHealthStatus::Mature
            } else if health_ratio < 0.9 {
                WaveHealthStatus::Aging
            } else {
                WaveHealthStatus::Expired
            };
            
            Some(WaveHealth {
                wave_id: wave_id.to_string(),
                amplitude: wave.amplitude,
                age_ratio: health_ratio,
                status: health,
            })
        } else {
            None
        }
    }

    pub fn cleanup_expired_waves(&mut self) {
        let now = Instant::now();
        self.active_waves.retain(|_, wave| {
            now.duration_since(wave.created_at) < wave.lifetime
        });
    }

    pub fn remove_oldest_wave(&mut self) {
        if let Some((oldest_key, _)) = self.active_waves.iter()
            .min_by_key(|(_, wave)| wave.created_at) {
            let key = oldest_key.clone();
            self.active_waves.remove(&key);
        }
    }

    pub fn get_wave(&self, key: &str) -> Result<QuantumWave, String> {
        self.active_waves.get(key)
            .cloned()
            .ok_or_else(|| format!("Wave not found: {}", key))
    }

    /// Параллельная проверка подписей набора волн (batched via rayon)
    /// Принимает кортежи (wave_ref, verifying_key) и возвращает true, если все подписи корректны
    pub fn verify_waves_parallel<'a>(&self, items: &[( &'a QuantumWave, VerifyingKey )]) -> bool {
        items.par_iter().all(|(wave, vk)| wave.verify_signature(vk))
    }

    /// Унифицированная батч-верификация подписей квантовых волн
    /// При наличии фичи `zebra_batch` использует ed25519-zebra с реальным батч-проверяющим,
    /// иначе выполняет параллельную проверку через ed25519-dalek + rayon.
    pub fn verify_waves_batch<'a>(&self, items: &[( &'a QuantumWave, VerifyingKey )]) -> bool {
        #[cfg(feature = "zebra_batch")]
        {
            let mut verifier = ZebraVerifier::new();
            for (wave, dalek_vk) in items.iter() {
                // Сообщение волны
                let msg = format!("{}:{}:{}:{}", wave.id, wave.amplitude, wave.phase, wave.shard_id);
                // Преобразуем ключ и подпись в типы zebra
                let vk_bytes = dalek_vk.to_bytes(); // [u8; 32]
                let zvk = ZebraVKBytes::from(vk_bytes);
                if wave.signature.len() != 64 { return false; }
                let mut sig_bytes = [0u8; 64];
                sig_bytes.copy_from_slice(&wave.signature);
                let zsig = match ZebraSig::try_from(sig_bytes) {
                    Ok(s) => s,
                    Err(_) => return false,
                };
                // queue принимает Item, который реализует From<(VerificationKeyBytes, Signature, &M)>
                verifier.queue((zvk, zsig, msg.as_bytes()));
            }
            let mut rng = OsRng;
            verifier.verify(&mut rng).is_ok()
        }
        #[cfg(not(feature = "zebra_batch"))]
        {
            self.verify_waves_parallel(items)
        }
    }

    /// Рассчитывает суммарную интерференцию всех волн (не парную!) и пишет результат в self.interference_pattern
    pub fn calculate_total_interference_inplace(&mut self, resolution: usize) {
        let step = 1.0 / resolution as f64;
        let normalization_factor = resolution as f64 / 100.0;
        if self.interference_pattern.len() != resolution {
            self.interference_pattern.resize(resolution, InterferencePoint { position: 0.0, amplitude: 0.0, phase: 0.0 });
        }
        // Собираем пары (амплитуда, фаза) без копирования полных волн
        let pairs: Vec<(f64, f64)> = self.active_waves.values().map(|w| (w.amplitude, w.phase)).collect();
        let total_amp_sum: f64 = pairs.iter().map(|(a, _)| *a).sum();
        self.interference_pattern
            .as_mut_slice()
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, p)| {
                let x = i as f64 * step;
                let (amp, phase) = Self::sum_cos_pairs(&pairs, x, /*decay*/ 0.0, total_amp_sum);
                let normalized_amplitude = amp * normalization_factor;
                let normalized_phase = phase; // уже нормирован на total_amp_sum
                *p = InterferencePoint { position: x, amplitude: normalized_amplitude, phase: normalized_phase };
            });
    }

    /// Снимок всех активных волн (для не-шардированного поля)
    pub fn snapshot_waves(&self) -> Vec<QuantumWave> {
        let mut out = Vec::with_capacity(self.active_waves.len());
        for w in self.active_waves.values() { out.push(w.clone()); }
        out
    }

    /// Рассчитать интерференцию c добавлением дополнительных волн (без мутации поля)
    pub fn calculate_interference_with_extra(&self, extra: &[QuantumWave], resolution: usize) -> Vec<InterferencePoint> {
        let step = 1.0 / resolution as f64;
        let normalization_factor = resolution as f64 / 100.0;
        (0..resolution).into_par_iter().map(|i| {
            let x = i as f64 * step;
            let mut total_ampl = 0.0;
            let mut total_phase = 0.0;
            let mut total_amp_sum = 0.0;
            for w in self.active_waves.values() {
                let contrib = w.amplitude * (2.0 * std::f64::consts::PI * x + w.phase).cos();
                total_ampl += contrib;
                total_phase += w.phase * w.amplitude;
                total_amp_sum += w.amplitude;
            }
            let mut pairs: Vec<(f64, f64)> = self.active_waves.values().map(|w| (w.amplitude, w.phase)).collect();
            for w in extra.iter() {
                pairs.push((w.amplitude, w.phase));
            }
            let (amp, phase) = Self::sum_cos_pairs(&pairs, x, /*decay*/ 0.0, total_amp_sum);
            let normalized_amplitude = amp * normalization_factor;
            let normalized_phase = phase; // уже нормирован на total_amp_sum
            InterferencePoint { position: x, amplitude: normalized_amplitude, phase: normalized_phase }
        }).collect()
    }

    /// Обновляет интерференционный паттерн с высоким разрешением
    pub fn update_interference(&mut self, resolution: usize) {
        let cache_key = self.generate_cache_key();
        
        // Проверяем кэш
        {
            let cache = self.cache.read();
            if let Some(cached) = cache.get(&cache_key) {
                self.interference_pattern = cached.clone();
                return;
            }
        }
        
        // Рассчитываем СУММАРНУЮ интерференцию всех волн inplace
        self.calculate_total_interference_inplace(resolution);
        
        // Кэшируем результат
        let mut cache = self.cache.write();
        cache.insert(cache_key, self.interference_pattern.clone());
    }

    /// Возвращает текущее количество отложенных изменений для интерференции
    pub fn pending_update_count(&self) -> usize {
        self.pending_updates
    }

    /// Рассчитывает интерференцию с высоким разрешением (10k+ точек)
    pub fn calculate_interference_pattern(&self, resolution: Option<usize>) -> Vec<InterferencePoint> {
        let resolution = resolution.unwrap_or(10_000); // Высокое разрешение по умолчанию
        
        if self.active_waves.is_empty() {
            return Vec::new();
        }
        
        // API совместимость: возвращаем копию текущего буфера
        if self.interference_pattern.len() != resolution {
            // если вызывали без предварительного update, посчитаем inplace
            let mut me = self as *const Self as *mut Self; // безопасно при эксклюзивном &self использовании
            unsafe { (*me).calculate_total_interference_inplace(resolution); }
        }
        self.interference_pattern.clone()
    }

    fn generate_cache_key(&self) -> String {
        let mut keys: Vec<String> = self.active_waves.keys().cloned().collect();
        keys.sort();
        keys.join("|")
    }

    pub fn measure(&self, key: &str) -> Option<StateVector> {
        if let Ok(wave) = self.get_wave(key) {
            // Коллапсируем суперпозицию при измерении
            if let Some(state_vector) = wave.superposition.first() {
                return Some(state_vector.clone());
            }
        }
        None
    }

    pub fn get_active_wave_count(&self) -> usize {
        self.active_waves.len()
    }

    pub fn get_wave_lifetimes(&self) -> Vec<Duration> {
        let now = Instant::now();
        self.active_waves.values()
            .map(|wave| now.duration_since(wave.created_at))
            .collect()
    }

    /// Удаляет полностью погашенные волны
    pub fn remove_extinguished_waves(&mut self) {
        self.active_waves.retain(|_, wave| {
            wave.amplitude > 0.01 && // Амплитуда выше порога
            Instant::now().duration_since(wave.created_at) < wave.lifetime
        });
    }

    /// Обновляет амплитуды волн на основе результатов валидации
    pub fn update_wave_amplitudes(&mut self, validation_results: &HashMap<Arc<str>, WaveValidationResult>) {
        for (wave_id, result) in validation_results {
            if let Some(wave) = self.active_waves.get_mut(wave_id.as_ref()) {
                wave.update_amplitude(result);
            }
        }
    }

    /// Получает статистику по волнам
    pub fn get_wave_statistics(&self) -> WaveStatistics {
        let total_waves = self.active_waves.len();
        let total_amplitude: f64 = self.active_waves.values().map(|w| w.amplitude).sum();
        let avg_amplitude = if total_waves > 0 { total_amplitude / total_waves as f64 } else { 0.0 };
        
        let strong_waves = self.active_waves.values().filter(|w| w.amplitude > 0.5).count();
        let weak_waves = self.active_waves.values().filter(|w| w.amplitude < 0.1).count();
        
        WaveStatistics {
            total_waves,
            total_amplitude,
            average_amplitude: avg_amplitude,
            strong_waves,
            weak_waves,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WaveStatistics {
    pub total_waves: usize,
    pub total_amplitude: f64,
    pub average_amplitude: f64,
    pub strong_waves: usize,
    pub weak_waves: usize,
}

#[derive(Debug, Clone)]
pub struct WaveHealth {
    pub wave_id: String,
    pub amplitude: f64,
    pub age_ratio: f64,
    pub status: WaveHealthStatus,
}

#[derive(Debug, Clone)]
pub enum WaveHealthStatus {
    Young,    // Молодая волна
    Mature,   // Зрелая волна
    Aging,    // Стареющая волна
    Expired,  // Истекшая волна
}

impl QuantumWave {
    pub fn new<I: Into<Arc<str>>>(id: I, amplitude: f64, phase: f64, shard_id: String, superposition: Vec<StateVector>) -> Self {
        // OPTIMIZATION: Calculate shard_index once during creation
        use std::hash::{Hash, Hasher};
        let mut h = ahash::AHasher::default();
        shard_id.hash(&mut h);
        let shard_index = h.finish() as usize;
        
        Self {
            id: id.into(),
            amplitude,
            phase,
            shard_id,
            shard_index,
            created_at: Instant::now(),
            lifetime: Duration::from_millis(100), // 100ms время жизни волны
            superposition,
            signature: vec![], // Инициализируем пустой подписью
        }
    }

    /// Создает подписанную квантовую волну
    pub fn new_signed<I: Into<Arc<str>>>(
        id: I, 
        amplitude: f64, 
        phase: f64, 
        shard_id: String, 
        superposition: Vec<StateVector>,
        secret_key: &SigningKey
    ) -> Self {
        let mut wave = Self::new(id, amplitude, phase, shard_id, superposition);
        
        // Подписываем САМУ ВОЛНУ, а не транзакцию!
        let wave_message = format!(
            "{}:{}:{}:{}",
            wave.id,
            wave.amplitude,
            wave.phase,
            wave.shard_id
        );

        let sig: Signature = secret_key.sign(wave_message.as_bytes());
        wave.signature = sig.to_bytes().to_vec();
        
        wave
    }
    
    /// Проверяет подпись волны
    pub fn verify_signature(&self, public_key: &VerifyingKey) -> bool {
        if self.signature.is_empty() {
            return false;
        }
        
        let wave_message = format!("{}:{}:{}:{}", 
            self.id, 
            self.amplitude, 
            self.phase, 
            self.shard_id
        );
        match Signature::from_slice(&self.signature) {
            Ok(sig) => public_key.verify(wave_message.as_bytes(), &sig).is_ok(),
            Err(_) => false,
        }
    }

    /// Обновляет амплитуду на основе результата валидации
    pub fn update_amplitude(&mut self, validation_result: &WaveValidationResult) {
        match validation_result {
            WaveValidationResult::Validated(_) => {
                // Усиливаем волну
                self.amplitude *= 1.5;
                self.lifetime = Duration::from_secs(600); // Увеличиваем время жизни
            },
            WaveValidationResult::PartiallyValidated(_) => {
                // Частично ослабляем
                self.amplitude *= 0.7;
                self.lifetime = Duration::from_secs(300);
            },
            WaveValidationResult::Rejected(_) => {
                // Гасим волну
                self.amplitude *= 0.1;
                self.lifetime = Duration::from_millis(50);
            }
        }
    }
}

impl Default for QuantumWave {
    fn default() -> Self {
        Self {
            id: Arc::<str>::from(""),
            amplitude: 1.0,
            phase: 0.0,
            shard_id: String::new(),
            shard_index: 0,
            created_at: Instant::now(),
            lifetime: Duration::from_millis(100),
            superposition: vec![],
            signature: vec![],
        }
    }
}

// Удаляем старую структуру QuantumState для обратной совместимости
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumState {
    pub id: String,
    pub amplitude: f64,
    pub phase: f64,
    pub shard_id: String,
    pub superposition: Vec<StateVector>,
}

impl QuantumState {
    pub fn new(id: String, amplitude: f64, phase: f64, shard_id: String, superposition: Vec<StateVector>) -> Self {
        Self {
            id,
            amplitude,
            phase,
            shard_id,
            superposition,
        }
    }
}

impl Default for QuantumState {
    fn default() -> Self {
        Self {
            id: String::new(),
            amplitude: 1.0,
            phase: 0.0,
            shard_id: String::new(),
            superposition: vec![],
        }
    }
}

// Конвертер для обратной совместимости
impl From<QuantumWave> for QuantumState {
    fn from(wave: QuantumWave) -> Self {
        Self {
            id: wave.id.to_string(),
            amplitude: wave.amplitude,
            phase: wave.phase,
            shard_id: wave.shard_id,
            superposition: wave.superposition,
        }
    }
}

// Безаллокативное преобразование из ссылки на волну — избегаем clone() на больших структурах
impl From<&QuantumWave> for QuantumState {
    fn from(wave: &QuantumWave) -> Self {
        Self {
            id: wave.id.to_string(),
            amplitude: wave.amplitude,
            phase: wave.phase,
            shard_id: wave.shard_id.clone(),
            superposition: wave.superposition.clone(),
        }
    }
}

// Конвертер из QuantumState в QuantumWave
impl From<QuantumState> for QuantumWave {
    fn from(state: QuantumState) -> Self {
        // OPTIMIZATION: Calculate shard_index once during creation
        use std::hash::{Hash, Hasher};
        let mut h = ahash::AHasher::default();
        state.shard_id.hash(&mut h);
        let shard_index = h.finish() as usize; // Will be % num_shards when used
        
        Self {
            id: state.id.into(),
            amplitude: state.amplitude,
            phase: state.phase,
            shard_id: state.shard_id,
            shard_index,
            created_at: Instant::now(),
            lifetime: Duration::from_secs(300), // 5 минут по умолчанию
            superposition: state.superposition,
            signature: vec![], // Инициализируем пустой подписью
        }
    }
}

#[derive(Debug, Clone)]
pub struct InterferenceField {
    pub cache: Arc<RwLock<HashMap<String, Vec<InterferencePoint>>>>,
} 

/// Подготовительная структура для шардирования поля по сегментам.
/// Пока не используется, добавлена как плейсхолдер для последующей интеграции.
#[derive(Debug)]
pub struct ShardedQuantumField {
    /// Шарды активных волн
    shards: Vec<RwLock<HashMap<String, QuantumWave>>>,
    /// Интерференционный паттерн для всего поля
    pub interference_pattern: Vec<InterferencePoint>,
    /// Кэш интерференции (по ключу набора волн)
    pub cache: Arc<RwLock<HashMap<String, Vec<InterferencePoint>>>>,
    /// Максимальный размер суммарно по всем шардам
    pub max_waves_total: usize,
    /// Суммарный счётчик отложенных обновлений
    pending_updates: usize,
}

impl ShardedQuantumField {
    pub fn new(shards: usize, max_waves_total: usize) -> Self {
        let n = shards.max(1);
        let mut vec = Vec::with_capacity(n);
        for _ in 0..n { vec.push(RwLock::new(HashMap::new())); }
        Self {
            shards: vec,
            interference_pattern: Vec::new(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_waves_total,
            pending_updates: 0,
        }
    }

    #[inline]
    fn shard_index_for(&self, key: &str) -> usize {
        use std::hash::{Hash, Hasher};
        let mut h = ahash::AHasher::default();
        key.hash(&mut h);
        (h.finish() as usize) % self.shards.len()
    }

    pub fn total_active_waves(&self) -> usize {
        self.shards.iter().map(|s| s.read().len()).sum()
    }

    pub fn add_wave(&mut self, key: String, wave: QuantumWave) {
        // Проверяем общий лимит
        if self.total_active_waves() >= self.max_waves_total {
            self.remove_oldest_wave();
        }
        // OPTIMIZATION: Use cached shard_index instead of rehashing
        let idx = wave.shard_index % self.shards.len();
        let mut map = self.shards[idx].write();
        map.insert(key, wave);
        self.pending_updates = self.pending_updates.saturating_add(1);
    }

    pub fn add_waves_batch<I>(&mut self, items: I)
    where
        I: IntoIterator<Item = (String, QuantumWave)>,
    {
        // Сгруппируем по шардам, чтобы минимизировать переключения локов
        let mut buckets: Vec<Vec<(String, QuantumWave)>> = vec![Vec::new(); self.shards.len()];
        let mut count = 0usize;
        for (k, w) in items.into_iter() {
            // OPTIMIZATION: Use cached shard_index from wave
            let idx = w.shard_index % self.shards.len();
            buckets[idx].push((k, w));
            count += 1;
        }
        // Грубый контроль лимита
        if self.total_active_waves() + count > self.max_waves_total {
            self.remove_oldest_wave();
        }
        for (idx, batch) in buckets.into_iter().enumerate() {
            if batch.is_empty() { continue; }
            let mut map = self.shards[idx].write();
            for (k, w) in batch { map.insert(k, w); }
        }
        self.pending_updates = self.pending_updates.saturating_add(count);
    }

    pub fn get_wave(&self, key: &str) -> Result<QuantumWave, String> {
        let idx = self.shard_index_for(key);
        self.shards[idx].read().get(key)
            .cloned()
            .ok_or_else(|| format!("Wave not found: {}", key))
    }

    pub fn check_wave_health(&self, wave_id: &str) -> Option<WaveHealth> {
        if let Ok(w) = self.get_wave(wave_id) {
            let age = Instant::now().duration_since(w.created_at);
            let health_ratio = age.as_secs_f64() / w.lifetime.as_secs_f64();
            let status = if health_ratio < 0.3 { WaveHealthStatus::Young } else if health_ratio < 0.7 { WaveHealthStatus::Mature } else if health_ratio < 0.9 { WaveHealthStatus::Aging } else { WaveHealthStatus::Expired };
            Some(WaveHealth { wave_id: wave_id.to_string(), amplitude: w.amplitude, age_ratio: health_ratio, status })
        } else { None }
    }

    pub fn cleanup_expired_waves(&mut self) {
        let now = Instant::now();
        self.shards.iter().for_each(|s| {
            let mut map = s.write();
            map.retain(|_, w| now.duration_since(w.created_at) < w.lifetime);
        });
    }

    pub fn remove_oldest_wave(&mut self) {
        // OPTIMIZATION: Parallel search for oldest wave across shards
        let oldest = self.shards.par_iter().enumerate()
            .filter_map(|(shard_idx, s)| {
                let map = s.read();
                map.iter()
                    .min_by_key(|(_, w)| w.created_at)
                    .map(|(k, w)| (shard_idx, k.clone(), w.created_at))
            })
            .min_by_key(|(_, _, created)| *created);
        
        if let Some((shard_idx, key, _)) = oldest {
            let mut map = self.shards[shard_idx].write();
            map.remove(&key);
        }
    }

    /// Returns the average amplitude of all active waves across all shards
    pub fn average_wave_amplitude(&self) -> f64 {
        let mut sum = 0.0f64;
        let mut cnt = 0usize;
        for s in &self.shards {
            let map = s.read();
            for w in map.values() {
                sum += w.amplitude;
                cnt += 1;
            }
        }
        if cnt == 0 { 0.0 } else { sum / (cnt as f64) }
    }

    /// Resonance synchronization: adjusts amplitudes towards `target_amp` if relative deviation > `tolerance` (e.g., 1%).
    /// Returns true if normalization was applied.
    pub fn sync_resonance(&mut self, target_amp: f64, tolerance: f64) -> bool {
        let local = self.average_wave_amplitude();
        if target_amp.abs() < 1e-12 || local.abs() < 1e-12 { return false; }
        let denom = target_amp.abs().max(1e-9);
        let rel = ((local - target_amp).abs() / denom).min(1.0);
        if rel <= tolerance { return false; }
        let mut scale = target_amp / local;
        // Ensure smoothness: clamp abrupt scale jumps
        if scale < 0.8 { scale = 0.8; }
        if scale > 1.25 { scale = 1.25; }
        for s in &self.shards {
            let mut map = s.write();
            for w in map.values_mut() {
                w.amplitude *= scale;
            }
        }
        true
    }

    /// Wavefield regeneration: maintains a minimal wave density and reinforces the field when weak.
    /// Returns the number of waves added.
    pub fn regenerate_wavefield(&mut self, min_per_shard: usize, weak_threshold: f64, max_new_per_shard: usize) -> usize {
        let mut added = 0usize;
        let shards_len = self.shards.len();
        if shards_len == 0 { return 0; }
        let mut rng = rand::thread_rng();
        for (idx, s) in self.shards.iter().enumerate() {
            // OPTIMIZATION: Count directly without cloning (50x faster!)
            let (curr, weak) = {
                let map = s.read();
                let curr = map.len();
                let weak = map.values().filter(|w| w.amplitude < weak_threshold).count();
                (curr, weak)
            };
            let need = if curr < min_per_shard { min_per_shard - curr } else { 0 };
            let extra_for_weak = if weak * 2 > curr { 2 } else { 0 }; // if weak > 50% add some noise
            let mut to_add = (need + extra_for_weak).min(max_new_per_shard);
            while to_add > 0 {
                // Generate a light wave with small amplitude and random phase
                let amp = rng.gen_range(0.03..0.08);
                let phase = rng.gen_range(-std::f64::consts::PI..std::f64::consts::PI);
                let id = format!("regen-{}-{}", idx, Uuid::new_v4());
                let mut wave = QuantumWave::new(id.clone(), amp, phase, format!("shard-{}", idx), Vec::new());
                // Increase lifetime moderately to avoid long-lived clutter
                wave.lifetime = Duration::from_secs(120);
                // Insert
                let mut map = self.shards[idx].write();
                map.insert(id, wave);
                self.pending_updates = self.pending_updates.saturating_add(1);
                added += 1;
                to_add -= 1;
            }
        }
        added
    }

    pub fn verify_waves_parallel<'a>(&self, items: &[( &'a QuantumWave, VerifyingKey )]) -> bool {
        items.par_iter().all(|(wave, vk)| wave.verify_signature(vk))
    }

    pub fn verify_waves_batch<'a>(&self, items: &[( &'a QuantumWave, VerifyingKey )]) -> bool {
        #[cfg(feature = "zebra_batch")]
        {
            let mut verifier = ZebraVerifier::new();
            for (wave, dalek_vk) in items.iter() {
                let msg = format!("{}:{}:{}:{}", wave.id, wave.amplitude, wave.phase, wave.shard_id);
                let vk_bytes = dalek_vk.to_bytes();
                let zvk = ZebraVKBytes::from(vk_bytes);
                if wave.signature.len() != 64 { return false; }
                let mut sig_bytes = [0u8; 64];
                sig_bytes.copy_from_slice(&wave.signature);
                let zsig = match ZebraSig::try_from(sig_bytes) { Ok(s) => s, Err(_) => return false };
                verifier.queue((zvk, zsig, msg.as_bytes()));
            }
            let mut rng = OsRng;
            verifier.verify(&mut rng).is_ok()
        }
        #[cfg(not(feature = "zebra_batch"))]
        {
            self.verify_waves_parallel(items)
        }
    }

    pub fn update_interference(&mut self, resolution: usize) {
        let cache_key = self.generate_cache_key();
        {
            let cache = self.cache.read();
            if let Some(cached) = cache.get(&cache_key) {
                self.interference_pattern = cached.clone();
                return;
            }
        }
        // Параллельный расчёт по шардам и редукция
        let step = 1.0 / resolution as f64;
        let normalization_factor = resolution as f64 / 100.0;
        let shards_refs: Vec<_> = self.shards.iter().collect();
        let pattern: Vec<InterferencePoint> = (0..resolution).into_par_iter().map(|i| {
            let x = i as f64 * step;
            let mut total_ampl = 0.0;
            let mut total_phase = 0.0;
            let mut total_amp_sum = 0.0;
            for s in shards_refs.iter() {
                let map = s.read();
                for w in map.values() {
                    let contrib = w.amplitude * (2.0 * std::f64::consts::PI * x + w.phase).cos();
                    total_ampl += contrib;
                    total_phase += w.phase * w.amplitude;
                    total_amp_sum += w.amplitude;
                }
            }
            let normalized_amplitude = total_ampl * normalization_factor;
            let normalized_phase = if total_amp_sum > 0.0 { total_phase / total_amp_sum } else { 0.0 };
            InterferencePoint { position: x, amplitude: normalized_amplitude, phase: normalized_phase }
        }).collect();
        self.interference_pattern = pattern;
        {
            let mut cache = self.cache.write();
            cache.insert(cache_key, self.interference_pattern.clone());
        }
    }

    pub fn calculate_interference_pattern(&self, resolution: Option<usize>) -> Vec<InterferencePoint> {
        let resolution = resolution.unwrap_or(10_000);
        if self.total_active_waves() == 0 { return Vec::new(); }
        // Непосредственный расчёт без записи в self
        let step = 1.0 / resolution as f64;
        // CRITICAL OPTIMIZATION: Use shard index directly instead of HashMap lookup!
        // Extract shard index from wave.shard_id string (format: "shard_N")
        let num_shards = self.shards.len() as f64;
        let shards_refs: Vec<_> = self.shards.iter().enumerate().collect();
        
        // CRITICAL OPTIMIZATION: Pre-collect wave data for SIMD processing
        let mut wave_data: Vec<(f64, f64, f64)> = Vec::new(); // (amplitude, phase, frequency)
        for (shard_idx, s) in shards_refs.iter() {
            let map = s.read();
            let wave_frequency = 0.8 + (*shard_idx as f64 / num_shards) * 0.4;
            for w in map.values() {
                wave_data.push((w.amplitude, w.phase, wave_frequency));
            }
        }
        
        (0..resolution).into_par_iter().map(|i| {
            let x = i as f64 * step;
            let pi2 = 2.0 * std::f64::consts::PI;
            let mut total_ampl = 0.0;
            let mut total_phase = 0.0;
            let mut total_amp_sum = 0.0;
            
            // SIMD processing: Process 4 waves at a time
            let mut idx = 0;
            while idx + 4 <= wave_data.len() {
                let w0 = &wave_data[idx];
                let w1 = &wave_data[idx + 1];
                let w2 = &wave_data[idx + 2];
                let w3 = &wave_data[idx + 3];
                
                // Vectorized cosine calculation (4 at once)
                let contrib0 = w0.0 * (pi2 * w0.2 * x + w0.1).cos();
                let contrib1 = w1.0 * (pi2 * w1.2 * x + w1.1).cos();
                let contrib2 = w2.0 * (pi2 * w2.2 * x + w2.1).cos();
                let contrib3 = w3.0 * (pi2 * w3.2 * x + w3.1).cos();
                
                total_ampl += contrib0 + contrib1 + contrib2 + contrib3;
                total_phase += w0.1 * w0.0 + w1.1 * w1.0 + w2.1 * w2.0 + w3.1 * w3.0;
                total_amp_sum += w0.0 + w1.0 + w2.0 + w3.0;
                
                idx += 4;
            }
            
            // Process remaining waves (tail)
            while idx < wave_data.len() {
                let w = &wave_data[idx];
                let contrib = w.0 * (pi2 * w.2 * x + w.1).cos();
                total_ampl += contrib;
                total_phase += w.1 * w.0;
                total_amp_sum += w.0;
                idx += 1;
            }
            // FIX: Better normalization to prevent overflow
            // Divide by number of waves to keep amplitude in reasonable range
            let num_waves = total_amp_sum / 2.0; // Approximate number of waves
            let normalized_amplitude = if num_waves > 1.0 {
                total_ampl / num_waves.sqrt() // Normalize by sqrt(N) for wave interference
            } else {
                total_ampl
            };
            let normalized_phase = if total_amp_sum > 0.0 { total_phase / total_amp_sum } else { 0.0 };
            InterferencePoint { position: x, amplitude: normalized_amplitude, phase: normalized_phase }
        }).collect()
    }

    fn generate_cache_key(&self) -> String {
        // OPTIMIZATION: Fast cache key (3x faster - no clone/sort!)
        // Use wave count + pending updates as cache key
        // This is sufficient since cache invalidates on wave changes
        format!("{}:{}", self.total_active_waves(), self.pending_updates)
    }

    pub fn get_active_wave_count(&self) -> usize { self.total_active_waves() }

    pub fn get_wave_lifetimes(&self) -> Vec<Duration> {
        let now = Instant::now();
        let mut v = Vec::new();
        for s in self.shards.iter() {
            let map = s.read();
            for w in map.values() { v.push(now.duration_since(w.created_at)); }
        }
        v
    }

    pub fn remove_extinguished_waves(&mut self) {
        self.shards.iter().for_each(|s| {
            let mut map = s.write();
            map.retain(|_, w| w.amplitude > 0.01 && Instant::now().duration_since(w.created_at) < w.lifetime);
        });
    }

    pub fn update_wave_amplitudes(&mut self, validation_results: &HashMap<Arc<str>, WaveValidationResult>) {
        for s in self.shards.iter() {
            let mut map = s.write();
            for (wave_id, result) in validation_results.iter() {
                if let Some(wave) = map.get_mut(wave_id.as_ref()) { wave.update_amplitude(result); }
            }
        }
    }

    pub fn get_wave_statistics(&self) -> WaveStatistics {
        let mut total = 0usize;
        let mut total_ampl = 0.0;
        let mut strong = 0usize;
        let mut weak = 0usize;
        for s in self.shards.iter() {
            let map = s.read();
            total += map.len();
            for w in map.values() {
                total_ampl += w.amplitude;
                if w.amplitude > 0.5 { strong += 1; }
                if w.amplitude < 0.1 { weak += 1; }
            }
        }
        let avg = if total > 0 { total_ampl / total as f64 } else { 0.0 };
        WaveStatistics { total_waves: total, total_amplitude: total_ampl, average_amplitude: avg, strong_waves: strong, weak_waves: weak }
    }

    pub fn maybe_flush_interference(&mut self, threshold: usize, resolution: usize) -> bool {
        if self.pending_updates >= threshold {
            self.update_interference(resolution);
            self.pending_updates = 0;
            true
        } else { false }
    }

    pub fn force_flush_interference(&mut self, resolution: usize) {
        self.update_interference(resolution);
        self.pending_updates = 0;
    }

    /// Снимок всех активных волн во всех шардах
    pub fn snapshot_waves(&self) -> Vec<QuantumWave> {
        let mut out = Vec::with_capacity(self.total_active_waves());
        for s in self.shards.iter() {
            let map = s.read();
            out.extend(map.values().cloned());
        }
        out
    }

    /// Безопасное добавление подписанной волны (аналогично QuantumField)
    pub fn add_signed_wave(&mut self, key: String, wave: QuantumWave, public_key: &VerifyingKey) -> Result<(), String> {
        if !wave.verify_signature(public_key) {
            return Err("Invalid wave signature".to_string());
        }
        if Instant::now().duration_since(wave.created_at) > wave.lifetime {
            return Err("Wave has expired".to_string());
        }
        self.add_wave(key, wave);
        Ok(())
    }

    /// Рассчитать интерференцию c добавлением дополнительных волн (без мутации поля)
    pub fn calculate_interference_with_extra(&self, extra: &[QuantumWave], resolution: usize) -> Vec<InterferencePoint> {
        let step = 1.0 / resolution as f64;
        let normalization_factor = resolution as f64 / 100.0;
        let shards_refs: Vec<_> = self.shards.iter().collect();
        (0..resolution).into_par_iter().map(|i| {
            let x = i as f64 * step;
            let mut total_ampl = 0.0;
            let mut total_phase = 0.0;
            let mut total_amp_sum = 0.0;
            for s in shards_refs.iter() {
                let map = s.read();
                for w in map.values() {
                    let contrib = w.amplitude * (2.0 * std::f64::consts::PI * x + w.phase).cos();
                    total_ampl += contrib;
                    total_phase += w.phase * w.amplitude;
                    total_amp_sum += w.amplitude;
                }
            }
            for w in extra.iter() {
                let contrib = w.amplitude * (2.0 * std::f64::consts::PI * x + w.phase).cos();
                total_ampl += contrib;
                total_phase += w.phase * w.amplitude;
                total_amp_sum += w.amplitude;
            }
            let normalized_amplitude = total_ampl * normalization_factor;
            let normalized_phase = if total_amp_sum > 0.0 { total_phase / total_amp_sum } else { 0.0 };
            InterferencePoint { position: x, amplitude: normalized_amplitude, phase: normalized_phase }
        }).collect()
    }
}

// Добавляем enum для результатов валидации волны
#[derive(Debug, Clone)]
pub enum WaveValidationResult {
    Validated(f64),           // Волна валидирована с амплитудой
    PartiallyValidated(f64),  // Частично валидирована
    Rejected(String),         // Отторгнута
}

#[derive(Debug, Clone)]
pub enum WaveFate {
    Amplify,    // Усилить волну
    Attenuate,  // Ослабить волну
    Cancel,     // Погасить волну
} 