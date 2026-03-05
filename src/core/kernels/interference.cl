// OpenCL kernel for Core interference calculation with primitive types

__constant double PI = 3.14159265358979323846;

__kernel void calculate_interference(
    __global const double* amplitudes,
    __global const double* phases,
    __global double* result,
    const uint num_states,
    const uint resolution,
    const double step
) {
    uint idx = get_global_id(0);
    if (idx >= resolution) return;

    double x = (double)idx * step;
    double total_amplitude = 0.0;

    for (uint i = 0; i < num_states; i++) {
        double amp = amplitudes[i];
        double phase = phases[i];
        total_amplitude += amp * cos(2.0 * PI * x + phase);
    }

    result[idx] = total_amplitude;
}

__kernel void calculate_interference_optimized(
    __global const double* amplitudes,
    __global const double* phases,
    __global double* result,
    __local double* local_buffer,
    const uint num_states,
    const uint resolution,
    const double step,
    const double normalization_factor
) {
    uint gid = get_global_id(0);
    uint lid = get_local_id(0);
    uint lsize = get_local_size(0);

    if (gid >= resolution) return;

    double x = (double)gid * step;
    double sum = 0.0;

    // Векторизованная обработка с использованием SIMD
    #pragma unroll 4
    for (uint base = 0; base < num_states; base += lsize) {
        uint idx = base + lid;
        if (idx < num_states) {
            double amp = amplitudes[idx];
            double phase = phases[idx];
            // Используем fast_math для ускорения тригонометрии
            local_buffer[lid] = amp * cos(2.0 * PI * x + phase);
        } else {
            local_buffer[lid] = 0.0;
        }
        barrier(CLK_LOCAL_MEM_FENCE);

        // Оптимизированная редукция с использованием tree reduction
        for (uint offset = lsize >> 1; offset > 0; offset >>= 1) {
            if (lid < offset) {
                local_buffer[lid] += local_buffer[lid + offset];
            }
            barrier(CLK_LOCAL_MEM_FENCE);
        }
        if (lid == 0) {
            sum += local_buffer[0];
        }
        barrier(CLK_LOCAL_MEM_FENCE);
    }

    // Применяем нормализацию амплитуды
    if (lid == 0) {
        result[gid] = sum * normalization_factor;
    }
}

__kernel void analyze_interference(
    __global const double* amplitudes,
    __global uint* constructive_count,
    __global uint* destructive_count,
    __global uint* max_amplitude_bits,  // Изменено на uint для атомарных операций
    __global uint* min_amplitude_bits,  // Изменено на uint для атомарных операций
    __global uint* sum_amplitude_bits,  // Изменено на uint для атомарных операций
    const uint resolution,
    const double threshold
) {
    uint idx = get_global_id(0);
    if (idx >= resolution) return;

    double a = amplitudes[idx];

    if (a > threshold) atomic_inc(constructive_count);
    else if (a < -threshold) atomic_inc(destructive_count);

    // Конвертируем double в uint для атомарных операций
    ulong a_bits = as_ulong(a);
    uint a_bits_high = (uint)(a_bits >> 32);
    uint a_bits_low = (uint)(a_bits & 0xFFFFFFFF);
    
    // Используем атомарные операции с целыми типами
    // Для double используем два uint (high и low части)
    atomic_max(max_amplitude_bits, a_bits_high);
    atomic_min(min_amplitude_bits, a_bits_low);
    atomic_add(sum_amplitude_bits, a_bits_high + a_bits_low);
}

// Новый высокопроизводительный kernel для batch обработки
__kernel void calculate_interference_batch(
    __global const double* amplitudes,
    __global const double* phases,
    __global double* result,
    __local double* local_buffer,
    const uint num_states,
    const uint resolution,
    const uint batch_size,
    const double step,
    const double normalization_factor
) {
    uint gid = get_global_id(0);
    uint lid = get_local_id(0);
    uint lsize = get_local_size(0);
    uint batch_id = gid / batch_size;
    uint local_idx = gid % batch_size;

    if (gid >= resolution) return;

    double x = (double)local_idx * step;
    double sum = 0.0;

    // Обрабатываем несколько состояний одновременно
    #pragma unroll 8
    for (uint base = 0; base < num_states; base += lsize) {
        uint idx = base + lid;
        if (idx < num_states) {
            double amp = amplitudes[idx];
            double phase = phases[idx];
            // Используем стандартный cos вместо native_cos для совместимости
            local_buffer[lid] = amp * cos(2.0 * PI * x + phase);
        } else {
            local_buffer[lid] = 0.0;
        }
        barrier(CLK_LOCAL_MEM_FENCE);

        // Оптимизированная редукция
        for (uint offset = lsize >> 1; offset > 0; offset >>= 1) {
            if (lid < offset) {
                local_buffer[lid] += local_buffer[lid + offset];
            }
            barrier(CLK_LOCAL_MEM_FENCE);
        }
        if (lid == 0) {
            sum += local_buffer[0];
        }
        barrier(CLK_LOCAL_MEM_FENCE);
    }

    // Применяем нормализацию и сохраняем результат
    if (lid == 0) {
        result[gid] = sum * normalization_factor;
    }
}

// ВЕКТОРИЗОВАННЫЙ KERNEL для Intel GPU - обрабатывает 4 точки одновременно
__kernel void calculate_interference_vectorized(
    __global const float* amplitudes,
    __global const float* phases,
    __global float4* result,
    uint num_states,
    uint resolution,
    float step
) {
    uint gid = get_global_id(0);
    uint lid = get_local_id(0);
    
    if (gid >= resolution / 4) return; // Обрабатываем по 4 точки
    
    // Вычисляем 4 позиции одновременно
    float4 x = (float4)(
        gid * 4 * step,
        (gid * 4 + 1) * step,
        (gid * 4 + 2) * step,
        (gid * 4 + 3) * step
    );
    
    float4 total_amplitude = (float4)(0.0f, 0.0f, 0.0f, 0.0f);
    float4 total_phase = (float4)(0.0f, 0.0f, 0.0f, 0.0f);
    
    // Векторизованный проход по состояниям
    for (uint i = 0; i < num_states; i++) {
        float amp = amplitudes[i];
        float phase = phases[i];
        
        // Векторизованное вычисление интерференции для 4 точек
        float4 contribution = (float4)(
            amp * cos(2.0f * PI * x.x + phase),
            amp * cos(2.0f * PI * x.y + phase),
            amp * cos(2.0f * PI * x.z + phase),
            amp * cos(2.0f * PI * x.w + phase)
        );
        total_amplitude += contribution;
        total_phase += (float4)(phase, phase, phase, phase);
    }
    
    // Сохраняем результат для 4 точек
    result[gid] = total_amplitude;
}

// ВЕКТОРИЗОВАННЫЙ KERNEL для анализа интерференции
__kernel void analyze_interference_vectorized(
    __global const float4* amplitudes,
    __global uint* constructive_count,
    __global uint* destructive_count,
    __global uint* max_amplitude_bits,  // Изменено на uint для атомарных операций
    __global uint* min_amplitude_bits,  // Изменено на uint для атомарных операций
    __global uint* sum_amplitude_bits,  // Изменено на uint для атомарных операций
    uint resolution,
    float threshold
) {
    uint gid = get_global_id(0);
    
    if (gid >= resolution / 4) return;
    
    float4 amp = amplitudes[gid];
    
    // Подсчет для 4 точек (с атомарными операциями для NVIDIA T4)
    if (amp.x > threshold) atomic_inc(constructive_count);
    if (amp.x < -threshold) atomic_inc(destructive_count);
    if (amp.y > threshold) atomic_inc(constructive_count);
    if (amp.y < -threshold) atomic_inc(destructive_count);
    if (amp.z > threshold) atomic_inc(constructive_count);
    if (amp.z < -threshold) atomic_inc(destructive_count);
    if (amp.w > threshold) atomic_inc(constructive_count);
    if (amp.w < -threshold) atomic_inc(destructive_count);
    
    // Обновляем статистику (с атомарными операциями для целых типов)
    float max_val = fmax(fmax(amp.x, amp.y), fmax(amp.z, amp.w));
    float min_val = fmin(fmin(amp.x, amp.y), fmin(amp.z, amp.w));
    float sum_val = amp.x + amp.y + amp.z + amp.w;
    
    // Конвертируем float в uint для атомарных операций
    uint max_bits = as_uint(max_val);
    uint min_bits = as_uint(min_val);
    uint sum_bits = as_uint(sum_val);
    
    // Используем атомарные операции с целыми типами
    atomic_max(max_amplitude_bits, max_bits);
    atomic_min(min_amplitude_bits, min_bits);
    atomic_add(sum_amplitude_bits, sum_bits);
}



