"""Apply isolated mechanical prototypes to the dedicated experiment worktree.

Always restore the listed sources from the pinned revision first. Do not run
against a checkout containing unrelated edits to these files.
"""
from pathlib import Path
import subprocess

BASE = '61c2875'
FILES = ['src/movement/types.rs', 'src/movement/config.rs', 'src/movement/generator.rs',
         'src/game_state.rs', 'src/attack_utils.rs', 'src/search.rs']


def change(root, file, before, after):
    p = root / file
    s = p.read_text()
    assert s.count(before) == 1, (file, before[:90], s.count(before))
    p.write_text(s.replace(before, after))


def replace_function(root, file, name, body):
    p = root / file
    s = p.read_text()
    start = s.index('fn ' + name + '(')
    a = s.index('{', start)
    end = balanced_end(s, a)
    p.write_text(s[:a] + '{\n' + body + '\n}' + s[end:])


def balanced_end(s, start):
    count = 0
    for i in range(start, len(s)):
        if s[i] == '{': count += 1
        if s[i] == '}':
            count -= 1
            if count == 0: return i + 1
    raise ValueError('unbalanced source')


def bitset(root):
    p = root / 'src/movement/types.rs'
    s = p.read_text().replace('use std::collections::HashSet;', '''/// Experimental replacement for the immutable capturing-ray blocker sets.
#[derive(Clone)]
pub struct PieceTypeSet {
    bits: [u64; (crate::piece::PieceType::SwordGeneral as usize + 64) / 64],
}
// NNUE two-step channel names use movement Debug text. Preserve HashSet's
// set-shaped formatting, especially the empty `{}` in existing templates.
impl std::fmt::Debug for PieceTypeSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}
impl PieceTypeSet {
    pub fn new() -> Self {
        Self { bits: [0; (crate::piece::PieceType::SwordGeneral as usize + 64) / 64] }
    }
    pub fn insert(&mut self, piece: PieceType) -> bool {
        let i = piece as usize;
        let mask = 1u64 << (i % 64);
        let fresh = self.bits[i / 64] & mask == 0;
        self.bits[i / 64] |= mask;
        fresh
    }
    pub fn contains(&self, piece: &PieceType) -> bool {
        let i = *piece as usize;
        self.bits[i / 64] & (1u64 << (i % 64)) != 0
    }
    pub fn iter(&self) -> impl Iterator<Item = &'static PieceType> + '_ {
        crate::eval::ALL_PIECE_TYPES.iter().filter(move |p| self.contains(p))
    }
}''').replace('HashSet<PieceType>', 'PieceTypeSet')
    p.write_text(s)
    p = root / 'src/movement/config.rs'
    s = p.read_text().replace('std::collections::HashSet::new()', 'PieceTypeSet::new()')
    s = s.replace('use std::collections::HashSet;', 'use crate::movement::types::PieceTypeSet;')
    s = s.replace('HashSet<PieceType>', 'PieceTypeSet').replace('HashSet::new()', 'PieceTypeSet::new()')
    p.write_text(s)
    change(root, 'src/movement/generator.rs', '&std::collections::HashSet<crate::piece::PieceType>',
           '&crate::movement::types::PieceTypeSet')


def props(root):
    p = root / 'src/movement/config.rs'
    s = p.read_text()
    # Route all literal configs through their constructor, preserving capabilities.
    import re
    for m in reversed(list(re.finditer(r'MovementConfig \{\s*capabilities:', s))):
        brace = s.index('{', m.start())
        end = balanced_end(s, brace)
        caps = s[s.index(':', brace)+1:end-1].strip().rstrip(',')
        s = s[:m.start()] + 'MovementConfig::new(' + caps + ')' + s[end:]
    s = s.replace('    pub capabilities: Vec<MovementCapability>,', '''    pub capabilities: Vec<MovementCapability>,
    pub has_two_step: bool,
    pub uses_capturing: bool,
    pub only_capturing_range: bool,
    pub range_dirs: u8,''')
    s = s.replace('        MovementConfig { capabilities }', '''        let has_two_step = capabilities.iter().any(|c| matches!(c, MovementCapability::TwoStep { .. }));
        let mut uses_capturing = false;
        let mut other_range = false;
        let mut range_dirs = 0;
        for c in &capabilities {
            if let MovementCapability::Range { directions, blocking, .. } = c {
                range_dirs |= *directions;
                if *blocking == BlockingMode::Capturing { uses_capturing = true; }
                else { other_range = true; }
            }
        }
        MovementConfig { capabilities, has_two_step, uses_capturing,
            only_capturing_range: uses_capturing && !other_range, range_dirs }''')
    p.write_text(s)
    change(root, 'src/game_state.rs', '''            let has_two_step = config.capabilities.iter().any(|cap| {
                matches!(cap, crate::movement::types::MovementCapability::TwoStep { .. })
            });''', '            let has_two_step = config.has_two_step;')
    change(root, 'src/game_state.rs', '''        let uses_capturing = config.capabilities.iter().any(|cap| {
            if let crate::movement::MovementCapability::Range { blocking, .. } = cap {
                *blocking == crate::movement::BlockingMode::Capturing
            } else {
                false
            }
        });''', '        let uses_capturing = config.uses_capturing;')
    replace_function(root, 'src/attack_utils.rs', 'has_range_movement_in_direction', '''    let config = MovementConfig::for_piece(piece);
    direction_set_contains(adjust_directions_for_color(config.range_dirs, piece.color), direction)''')
    replace_function(root, 'src/attack_utils.rs', 'has_only_capturing_range_movement', '''    MovementConfig::for_piece(piece).only_capturing_range''')


def lazy_gates(root):
    p = root / 'src/search.rs'
    s = p.read_text()
    start = s.index('    // Q after loud AB captures/promos', s.index('fn leaf_or_quiesce'))
    end = s.index('        ctx.phase = "quiesce";', start)
    # Keep royal evasions first, and preserve every q-entry argument below.
    s = s[:start]+'''    let q = leaf_quiescence_depth(ctx, is_pv);
    if q == 0 { return evaluate_with_ply(state, weights, ctx.ply); }
    let capture_parent = ctx.last_ab_capture_enemy > 0.0;
    let include_caps = ctx.last_ab_capture_enemy >= min_quiescence_enemy_material()
        || (ctx.q_open_large_mover && ctx.last_ab_mover_large && capture_parent)
        || (ctx.q_open_any_capture && capture_parent)
        || (ctx.q_own_large_only && stm_has_dest_take_of_prev_large(state, ctx.last_ab_to))
        || stm_has_large_hang_take(state, weights, QHangOpts::from_ctx(ctx))
        || stm_has_royal_capture(state);
    if !include_caps && generate_loud_promotions(state).is_empty() {
        evaluate_with_ply(state, weights, ctx.ply)
    } else {
'''+s[end:]
    p.write_text(s)


def lazy_labels(root):
    p = root/'src/search.rs'
    s = p.read_text()
    s = s.replace('root_label: String', 'root_label: LazyMoveLabel').replace('q_label: String', 'q_label: LazyMoveLabel')
    s = s.replace('root_label: LazyMoveLabel::new()', 'root_label: LazyMoveLabel::default()').replace('q_label: LazyMoveLabel::new()', 'q_label: LazyMoveLabel::default()')
    s = s.replace('ctx.root_label = move_label(state, mv);', 'ctx.root_label = LazyMoveLabel::new(state, mv);')
    s = s.replace('ctx.q_label = move_label(state, &c.mv);', 'ctx.q_label = LazyMoveLabel::new(state, &c.mv);')
    s = s.replace('                if self.q_label.is_empty() {\n                    "-"\n                } else {\n                    &self.q_label\n                },', '                if self.q_label.0.is_none() { "-".to_string() } else { self.q_label.to_string() },')
    marker = 'fn move_label(state: &GameState, mv: &Move) -> String {'
    definition = """
#[derive(Clone, Copy, Default)]
struct LazyMoveLabel(Option<(Option<crate::piece::Piece>, crate::position::Position, crate::position::Position, bool)>);
impl LazyMoveLabel {
    fn new(state: &GameState, mv: &Move) -> Self {
        Self(Some((state.get_board().get_piece(mv.from), mv.from, mv.to, mv.promoted)))
    }
    fn clear(&mut self) { self.0 = None; }
}
impl std::fmt::Display for LazyMoveLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Some((piece, from, to, promoted)) = self.0 else { return Ok(()); };
        match piece {
            Some(p) => {
                if p.is_promoted { write!(f, "+")?; }
                write!(f, "{}", p.base_symbol())?;
            }
            None => write!(f, "?")?,
        }
        write!(f, " {},{}→{},{}{}", 36-from.file, 36-from.rank, 36-to.file, 36-to.rank, if promoted { "+" } else { "" })
    }
}
"""
    assert marker in s
    s = s.replace(marker, definition+'\n'+marker)
    p.write_text(s)


def append_tests(root, variant):
    if 'bitset' in variant:
        with (root/'src/movement/types.rs').open('a') as f:
            f.write("""
#[cfg(test)]
mod bitset_experiment_tests {
    use super::*;
    #[test]
    fn all_piece_sets_match_hashset() {
        for modulus in 1..=11 {
            let mut fast = PieceTypeSet::new();
            let mut old = std::collections::HashSet::new();
            for &p in crate::eval::ALL_PIECE_TYPES {
                if p as usize % modulus == 0 {
                    assert_eq!(fast.insert(p), old.insert(p));
                    assert_eq!(fast.insert(p), old.insert(p));
                }
            }
            for p in crate::eval::ALL_PIECE_TYPES { assert_eq!(fast.contains(p), old.contains(p)); }
            assert_eq!(fast.iter().copied().collect::<std::collections::HashSet<_>>(), old);
        }
    }
}
""")
    if 'props' in variant:
        with (root/'src/movement/config.rs').open('a') as f:
            f.write("""
#[cfg(test)]
mod props_experiment_tests {
    use super::*;
    #[test]
    fn all_config_metadata_matches_scans() {
        for &kind in crate::eval::ALL_PIECE_TYPES {
            for base in [None, Some(kind), Some(PieceType::ReverseChariot)] {
                let mut piece = crate::piece::Piece::new(kind, crate::piece::Color::Black, crate::position::Position::new(17,17).unwrap());
                piece.is_promoted = base.is_some();piece.base_piece_type = base;
                let c = MovementConfig::for_piece(&piece);
                assert_eq!(c.has_two_step, c.capabilities.iter().any(|x|matches!(x,MovementCapability::TwoStep{..})));
                let mut capture = false;let mut other = false;let mut dirs = 0;
                for cap in &c.capabilities {
                    if let MovementCapability::Range{directions,blocking,..}=cap {
                        dirs |= *directions;
                        if *blocking == BlockingMode::Capturing {capture=true;} else {other=true;}
                    }
                }
                assert_eq!(c.uses_capturing,capture);
                assert_eq!(c.only_capturing_range,capture&&!other);
                assert_eq!(c.range_dirs,dirs);
            }
        }
    }
}
""")

    if 'lazy-labels' in variant:
        with (root/'src/search.rs').open('a') as f:
            f.write("""
#[cfg(test)]
mod lazy_label_experiment_tests {
    use super::*;
    #[test]
    fn labels_match_and_survive_board_changes() {
        let from = crate::position::Position::new(4,6).unwrap();
        let to = crate::position::Position::new(5,7).unwrap();
        for &kind in crate::eval::ALL_PIECE_TYPES {
            for promoted in [false,true] {
                let mut state = GameState::new();state.clear_board();
                let mut piece = crate::piece::Piece::new(kind, Color::Black,from);
                piece.is_promoted=promoted;piece.base_piece_type=Some(crate::piece::PieceType::Pawn);
                state.place_piece(piece);
                for move_promotes in [false,true] {
                    let mut mv=Move::new(from,to);mv.promoted=move_promotes;
                    let text=move_label(&state,&mv);
                    let lazy=LazyMoveLabel::new(&state,&mv);
                    assert_eq!(lazy.to_string(),text);
                    let mut changed=state.clone();changed.clear_board();
                    assert_eq!(lazy.to_string(),text);
                }
            }
        }
        let state=GameState::new();
        let mv=Move::new(from,to);
        assert_eq!(LazyMoveLabel::new(&state,&mv).to_string(),move_label(&state,&mv));
        let mut label=LazyMoveLabel::new(&state,&mv);label.clear();assert_eq!(label.to_string(),"");
    }
}
""")


def apply(root, variant):
    root = Path(root)
    for f in FILES:
        (root/f).write_bytes(subprocess.check_output(['git', 'show', f'{BASE}:{f}'], cwd=root))
    if variant == 'baseline': return
    for item in variant.split('+'):
        {'bitset': bitset, 'bitset-compat': bitset, 'props': props, 'lazy-gates': lazy_gates, 'lazy-labels': lazy_labels}[item](root)
    append_tests(root, variant)


if __name__ == '__main__':
    import argparse
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('variant')
    p.add_argument('--root', type=Path, required=True)
    a = p.parse_args()
    apply(a.root, a.variant)
