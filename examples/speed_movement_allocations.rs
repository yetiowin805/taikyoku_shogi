use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use taikyoku_shogi::game_state::LegalMoveGen;
use taikyoku_shogi::notation::tsfen_decode;
struct Counter;
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(l.size(), Ordering::Relaxed);
        System.alloc(l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(n, Ordering::Relaxed);
        System.realloc(p, l, n)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
}
#[global_allocator]
static COUNTER: Counter = Counter;
fn main() {
    let mut fixtures = vec![("opening".to_owned(), tsfen_decode("startpos").unwrap())];
    if let Some(path) = std::env::args().nth(1) {
        for (i, line) in std::fs::read_to_string(path).unwrap().lines().enumerate() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            fixtures.push((
                format!("corpus-{i}"),
                tsfen_decode(v["tsfen"].as_str().unwrap()).unwrap(),
            ));
        }
    }
    for (name, pos) in fixtures {
        let state = pos.to_state().unwrap();
        for mode in [
            LegalMoveGen::All,
            LegalMoveGen::CapturesOnly,
            LegalMoveGen::WithoutQuietMultiLeg,
            LegalMoveGen::QuietMultiLegOnly,
        ] {
            let mut out = state.generate_legal_moves_mode(mode);
            for reuse in [false, true] {
                CALLS.store(0, Ordering::Relaxed);
                BYTES.store(0, Ordering::Relaxed);
                let count = if reuse {
                    state.generate_legal_moves_mode_into(mode, &mut out);
                    out.len()
                } else {
                    state.generate_legal_moves_mode(mode).len()
                };
                let calls = CALLS.load(Ordering::Relaxed);
                let bytes = BYTES.load(Ordering::Relaxed);
                println!(
                    "{}",
                    serde_json::json!({"case":name,"mode":format!("{mode:?}"),"reuse":reuse,"count":count,"calls":calls,"requested_bytes":bytes})
                );
            }
        }
    }
}
