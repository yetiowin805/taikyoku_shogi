//! Reproduce the direction-mask simplification without changing production code.
//!
//! Run `cargo run --release --example audit_direction_probe` for parity.
//! Emit assembly with `cargo rustc --release --example audit_direction_probe --
//! --emit=asm` and inspect the two exported `audit_direction_*` symbols.
use taikyoku_shogi::attack_utils::adjust_directions_for_color;
use taikyoku_shogi::movement::direction::Direction;
use taikyoku_shogi::piece::Color;

/// Exact White-branch body from `adjust_directions_for_color` at the audit base.
#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn audit_direction_current(directions: u8) -> u8 {
    let mut adjusted = 0u8;
    if directions & Direction::N.to_bit() != 0 {
        adjusted |= Direction::S.to_bit();
    }
    if directions & Direction::S.to_bit() != 0 {
        adjusted |= Direction::N.to_bit();
    }
    if directions & Direction::E.to_bit() != 0 {
        adjusted |= Direction::W.to_bit();
    }
    if directions & Direction::W.to_bit() != 0 {
        adjusted |= Direction::E.to_bit();
    }
    if directions & Direction::NE.to_bit() != 0 {
        adjusted |= Direction::SW.to_bit();
    }
    if directions & Direction::SW.to_bit() != 0 {
        adjusted |= Direction::NE.to_bit();
    }
    if directions & Direction::SE.to_bit() != 0 {
        adjusted |= Direction::NW.to_bit();
    }
    if directions & Direction::NW.to_bit() != 0 {
        adjusted |= Direction::SE.to_bit();
    }
    adjusted
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn audit_direction_rotated(directions: u8) -> u8 {
    directions.rotate_left(4)
}

fn main() {
    for mask in u8::MIN..=u8::MAX {
        assert_eq!(adjust_directions_for_color(mask, Color::Black), mask);
        assert_eq!(
            adjust_directions_for_color(mask, Color::White),
            audit_direction_rotated(mask),
        );
        assert_eq!(audit_direction_current(mask), audit_direction_rotated(mask),);
    }
    println!(
        "{}",
        serde_json::json!({
            "masks": 256,
            "production_color_checks": 512,
            "branch_body_checks": 256,
            "failures": 0,
        })
    );
}
