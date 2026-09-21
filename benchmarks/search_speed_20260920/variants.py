"""Small, auditable source transforms applied only in an archived build checkout."""
from pathlib import Path

def replace(path, old, new, count=None):
 s=path.read_text()
 assert old in s,(path,old[:80])
 if count is not None:assert s.count(old)==count,(path,s.count(old),count)
 path.write_text(s.replace(old,new))

def apply(work:Path, variant:str):
 if variant=='stock':return
 for part in variant.split('+'):
  search=work/'src/search.rs'
  if part in ['qcache','qcache-late']:
   replace(search,'    struct QCand {\n','    struct QCand {\n        last_royal_take: bool,\n',1)
   assignment='capture_takes_last_enemy_royal(state, &mv)' if part=='qcache' else 'false'
   replace(search,'            Some(QCand {\n                mv,',f'            let last_royal_take = {assignment};\n            Some(QCand {{\n                last_royal_take,\n                mv,',1)
   if part=='qcache-late':
    replace(search,'    // Last-royal (instant win), then loud promo, path-sum, dest recapture, net MVV-LVA.', '''    // Candidates have survived filtering; count royals only once for this node.
    if cands.len() > 1 {
    let royal_count = state.get_board().pieces_by_color(state.get_current_turn().opposite())
        .iter().filter(|p| p.piece_type.is_royal()).count();
    for candidate in &mut cands {
        candidate.last_royal_take = candidate.is_royal_take && royal_count > 0
            && (royal_count == 1 || captured_enemy_royal_count(state, &candidate.mv) >= royal_count);
    }
    }
    // Last-royal (instant win), then loud promo, path-sum, dest recapture, net MVV-LVA.''',1)
   replace(search,'let win_a = capture_takes_last_enemy_royal(state, &a.mv);','let win_a = a.last_royal_take;',1)
   replace(search,'let win_b = capture_takes_last_enemy_royal(state, &b.mv);','let win_b = b.last_royal_take;',1)
  elif part=='iter':
   p=work/'src/movement/direction.rs'
   with p.open('a') as f:f.write('''
/// Allocation-free direction walk with the same order as the Vec API.
pub fn direction_iter(set: DirectionSet) -> impl Iterator<Item = Direction> {
    [Direction::N, Direction::NE, Direction::E, Direction::SE,
     Direction::S, Direction::SW, Direction::W, Direction::NW]
        .into_iter().filter(move |d| direction_set_contains(set, *d))
}
''')
   p=work/'src/movement/generator.rs'
   replace(p,'direction_set_to_directions','direction_iter')
   p=work/'src/path_utils.rs'
   with p.open('a') as f:f.write('''
/// Lazy equivalent of get_path_positions, including its off-ray stopping rules.
pub fn path_positions(from: Position, to: Position) -> impl Iterator<Item = Position> {
    let df = (to.file as i8 - from.file as i8).signum();
    let dr = (to.rank as i8 - from.rank as i8).signum();
    let mut file = from.file as i8 + df;
    let mut rank = from.rank as i8 + dr;
    let mut done = df == 0 && dr == 0;
    std::iter::from_fn(move || {
        if done || (df != 0 && (file - to.file as i8) * df > 0)
            || (dr != 0 && (rank - to.rank as i8) * dr > 0) { return None; }
        let p = Position::new(file as u8, rank as u8)?;
        done = p == to;
        file += df; rank += dr;
        Some(p)
    })
}

#[cfg(test)]
mod iterator_parity {
    use super::*;
    #[test]
    fn every_square_pair_matches_old_path() {
        for a in 0..1296 { for b in 0..1296 {
            let from=Position::from_index(a).unwrap();let to=Position::from_index(b).unwrap();
            assert_eq!(get_path_positions(from,to),path_positions(from,to).collect::<Vec<_>>());
        }}
    }
    #[test]
    fn every_direction_mask_matches_old_order() {
        for mask in 0..=255 {
            assert_eq!(crate::movement::direction::direction_set_to_directions(mask),
                crate::movement::direction::direction_iter(mask).collect::<Vec<_>>());
        }
    }
}
''')
   for name in ['search.rs','game_state.rs','move_simulation.rs','movement/generator.rs']:
    p=work/'src'/name;replace(p,'path_utils::get_path_positions(','path_utils::path_positions(')
  elif part.startswith('localpool'):
   capacity=int(part[len('localpool'):])
   replace(search,'    killers: Vec<[Option<MoveKey>; 2]>,','    move_buffers: Vec<Vec<Move>>,\n    killers: Vec<[Option<MoveKey>; 2]>,',1)
   replace(search,'        killers: Vec::new(),','        move_buffers: Vec::new(),\n        killers: Vec::new(),')
   replace(search,'    let mut moves = state.generate_legal_moves_mode(LegalMoveGen::WithoutQuietMultiLeg);',f'    let mut moves = ctx.move_buffers.pop().unwrap_or_else(|| Vec::with_capacity({capacity}));\n    state.generate_legal_moves_mode_into(LegalMoveGen::WithoutQuietMultiLeg, &mut moves);',1)
   replace(search,'        moves = state.generate_legal_moves_mode(LegalMoveGen::QuietMultiLegOnly);','        state.generate_legal_moves_mode_into(LegalMoveGen::QuietMultiLegOnly, &mut moves);',1)
   replace(search,'        used_only_stage_b = true;\n        if moves.is_empty() {','        used_only_stage_b = true;\n        if moves.is_empty() {\n            ctx.move_buffers.push(moves);',1)
   replace(search,'        let mut stage_b = state.generate_legal_moves_mode(LegalMoveGen::QuietMultiLegOnly);',f'        let mut stage_b = ctx.move_buffers.pop().unwrap_or_else(|| Vec::with_capacity({capacity}));\n        state.generate_legal_moves_mode_into(LegalMoveGen::QuietMultiLegOnly, &mut stage_b);',1)
   replace(search,'            alpha = a2;\n        }\n    }','            alpha = a2;\n        }\n        stage_b.clear();\n        ctx.move_buffers.push(stage_b);\n    }\n    moves.clear();\n    ctx.move_buffers.push(moves);',1)
  elif part.startswith('pool'):
   capacity=int(part[4:])
   with search.open('a') as f:f.write('''
thread_local! {
    static MOVE_BUFFERS: std::cell::RefCell<Vec<Vec<Move>>> = const { std::cell::RefCell::new(Vec::new()) };
}
struct MoveBuffer(Vec<Move>);
impl MoveBuffer {
    fn generate(state: &GameState, mode: LegalMoveGen) -> Self {
        let mut moves = MOVE_BUFFERS.with(|pool| pool.borrow_mut().pop())
            .unwrap_or_else(|| Vec::with_capacity(CAPACITY));
        state.generate_legal_moves_mode_into(mode, &mut moves);
        Self(moves)
    }
}
impl std::ops::Deref for MoveBuffer { type Target = Vec<Move>; fn deref(&self) -> &Self::Target { &self.0 } }
impl std::ops::DerefMut for MoveBuffer { fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 } }
impl Drop for MoveBuffer {
    fn drop(&mut self) {
        let mut moves = std::mem::take(&mut self.0); moves.clear();
        MOVE_BUFFERS.with(|pool| { let mut pool=pool.borrow_mut(); if pool.len()<128 {pool.push(moves);} });
    }
}
'''.replace('CAPACITY',str(capacity)))
   for mode in ['WithoutQuietMultiLeg','QuietMultiLegOnly']:
    replace(search,f'state.generate_legal_moves_mode(LegalMoveGen::{mode})',f'MoveBuffer::generate(state, LegalMoveGen::{mode})')
  elif part in ['tt-clear','tt-dirty'] or part.startswith('tt-adaptive'):
   dirty=part!='tt-clear'
   # Return clean backing storage to a bounded thread-local pool. No bounds survive a search.
   replace(search,'struct TranspositionTable {\n','struct TranspositionTable {\n    touched: Vec<usize>,\n',1)
   replace(search,'            entries: vec![None; n],','            entries: take_tt_storage(n),\n            touched: Vec::new(),',1)
   if dirty:
    replace(search,'                self.entries[base] = Some(entry);','                if self.entries[base].is_none() { self.touched.push(base); }\n                self.entries[base] = Some(entry);',1)
    replace(search,'        self.entries[empty.unwrap_or(worst)] = Some(entry);','        let idx = empty.unwrap_or(worst);\n        if self.entries[idx].is_none() { self.touched.push(idx); }\n        self.entries[idx] = Some(entry);',1)
   clean='for i in self.touched.drain(..) { self.entries[i] = None; }' if dirty else 'self.entries.fill(None);'
   if part.startswith('tt-adaptive'):
    divisor=int(part[len('tt-adaptive'):]); assert divisor>0
    clean=f'if self.touched.len() > self.entries.len() / {divisor} {{ self.entries.fill(None); }} else {{ {clean} }}'
   with search.open('a') as f:f.write('''
thread_local! { static TT_STORAGE: std::cell::RefCell<Vec<Vec<Option<TtEntry>>>> = const {std::cell::RefCell::new(Vec::new())}; }
fn take_tt_storage(n: usize) -> Vec<Option<TtEntry>> {
    TT_STORAGE.with(|pool| { let mut pool=pool.borrow_mut();
        pool.iter().position(|v| v.len()==n).map(|i|pool.swap_remove(i))
    }).unwrap_or_else(||vec![None;n])
}
impl Drop for TranspositionTable {
    fn drop(&mut self) {
        CLEAN
        let entries=std::mem::take(&mut self.entries);
        TT_STORAGE.with(|pool| { let mut pool=pool.borrow_mut(); if pool.len()<8 {pool.push(entries);} });
    }
}
'''.replace('CLEAN',clean))
   with search.open('a') as f:f.write("""
#[cfg(test)]
mod reusable_tt_tests {
    use super::*;
    #[test]
    fn table_pool_never_retains_bounds_across_searches() {
        for clusters in [1, 2, 4, 8] {
            {
                let mut table = TranspositionTable::with_clusters(1024, clusters);
                for key in 0..64u64 {
                    for offset in [0, 1024, 2048, 0] {
                        table.store(TtEntry { key: key + offset, depth: 4,
                            score: 123, bound: TtBound::Exact, best: None });
                    }
                }
                assert!(table.entries.iter().any(Option::is_some));
            }
            let table = TranspositionTable::with_clusters(1024, clusters);
            assert!(table.entries.iter().all(Option::is_none));
        }
    }
}
""")
  else:raise ValueError(part)
