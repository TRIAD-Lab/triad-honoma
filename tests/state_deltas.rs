use std::thread;
use triad::tvm::state::{StateView, WriteBatch};

fn key(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

#[test]
fn state_deltas_increments() {
    let state = StateView::new();
    let n = 100usize;
    let mut handles = Vec::with_capacity(n);
    for _ in 0..n {
        handles.push(thread::spawn(|| {
            let mut wb = WriteBatch::new();
            wb.add_u128_le_increase(key("k"), 1);
            wb
        }));
    }
    for h in handles {
        state.put_batch(&h.join().unwrap());
    }
    let v = state.get(&key("k")).unwrap_or_default();
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&v[..16]);
    let got = u128::from_le_bytes(arr);
    assert_eq!(got, 100);
}

#[test]
fn state_deltas_inc_dec_mix() {
    let state = StateView::new();
    let inc = 70usize;
    let dec = 30usize;
    let mut handles = Vec::with_capacity(inc + dec);
    for _ in 0..inc {
        handles.push(thread::spawn(|| {
            let mut wb = WriteBatch::new();
            wb.add_u128_le_increase(key("m"), 1);
            wb
        }));
    }
    for _ in 0..dec {
        handles.push(thread::spawn(|| {
            let mut wb = WriteBatch::new();
            wb.add_u128_le_decrease(key("m"), 1);
            wb
        }));
    }
    for h in handles {
        state.put_batch(&h.join().unwrap());
    }
    let v = state.get(&key("m")).unwrap_or_default();
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&v[..16]);
    let got = u128::from_le_bytes(arr);
    assert_eq!(got, (inc - dec) as u128);
}

#[test]
fn state_deltas_saturating_underflow() {
    let state = StateView::new();
    let mut wb = WriteBatch::new();
    wb.add_u128_le_decrease(key("z"), 10);
    state.put_batch(&wb);
    let v = state.get(&key("z")).unwrap_or_default();
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&v[..16]);
    let got = u128::from_le_bytes(arr);
    assert_eq!(got, 0);
}

#[test]
fn state_deltas_ordering_put_overrides_deltas() {
    let state = StateView::new();
    // Apply some deltas
    let mut wb1 = WriteBatch::new();
    wb1.add_u128_le_increase(key("p"), 10);
    state.put_batch(&wb1);
    // Put should override final value when applied after deltas
    let mut wb2 = WriteBatch::new();
    wb2.add_u128_le_increase(key("p"), 5);
    wb2.put(key("p"), 5u128.to_le_bytes().to_vec());
    state.put_batch(&wb2);
    let v = state.get(&key("p")).unwrap_or_default();
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&v[..16]);
    let got = u128::from_le_bytes(arr);
    assert_eq!(got, 5);
}
