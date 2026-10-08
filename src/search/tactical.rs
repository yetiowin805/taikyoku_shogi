//! Off-default controls for reproducible tactical experiments. Never read per node.
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QTableMode { #[default] Normal, Hints, Context }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LmrMode { #[default] Normal, Evasions, Off, InteriorOff, RootOff }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StandPatMode { #[default] Normal, MajorCaptureRequired }

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TacticalOptions {
    pub q_tempo_percent: u32,
    pub dump_pv: bool,
    pub q_evasions: bool,
    pub q_tt: QTableMode,
    pub full_nonpv_q: bool,
    pub always_q: bool,
    pub q_all_captures: bool,
    pub q_no_delta: bool,
    pub q_delta_margin: i32,
    pub no_hang: bool,
    pub no_null: bool,
    /// Attempt every requested depth until the unchanged hard deadline.
    pub no_soft_stop: bool,
    pub lmr: LmrMode,
    pub stand_pat: StandPatMode,
    /// Exact notation::move_encode identities, including the entire route.
    pub roots: Vec<String>,
}

impl Default for TacticalOptions {
    fn default() -> Self { Self {
        q_tempo_percent: 100, dump_pv: false, q_evasions: false,
        q_tt: QTableMode::Normal, full_nonpv_q: false, always_q: false,
        q_all_captures: false, q_no_delta: false, q_delta_margin: 0,
        no_hang: false, no_null: false, no_soft_stop: false, lmr: LmrMode::Normal,
        stand_pat: StandPatMode::Normal, roots: Vec::new(),
    }}
}

impl TacticalOptions {
    /// Explicit opt-in by diagnostic callers only; production never reads these.
    pub fn from_env() -> Result<Self, String> {
        let vars: HashMap<String, String> = std::env::vars()
            .filter(|(k, _)| k.starts_with("TACTICAL_"))
            .collect();
        if !cfg!(feature = "search-experiments") && !vars.is_empty() {
            return Err("TACTICAL_* requires the search-experiments build feature".into());
        }
        Self::parse(&vars)
    }

    fn parse(vars: &HashMap<String, String>) -> Result<Self, String> {
        let mut out = Self::default();
        for (key, value) in vars {
            let boolean = || match value.as_str() {
                "0" => Ok(false), "1" => Ok(true),
                _ => Err(format!("{key} must be 0 or 1, got {value:?}")),
            };
            match key.as_str() {
                "TACTICAL_QDEPTH" => {
                    value.parse::<u32>().map_err(|_| format!("{key} must be u32"))?;
                }
                "TACTICAL_QBROAD" => { boolean()?; }
                "TACTICAL_Q_TEMPO_PERCENT" => {
                    out.q_tempo_percent = value.parse().map_err(|_| format!("invalid {key}"))?;
                    if out.q_tempo_percent > 100 { return Err(format!("{key} must be 0..100")); }
                }
                "TACTICAL_DUMP_PV" => out.dump_pv = boolean()?,
                "TACTICAL_Q_EVASIONS" => out.q_evasions = boolean()?,
                "TACTICAL_FULL_NONPV_Q" => out.full_nonpv_q = boolean()?,
                "TACTICAL_ALWAYS_Q" => out.always_q = boolean()?,
                "TACTICAL_Q_ALL_CAPTURES" => out.q_all_captures = boolean()?,
                "TACTICAL_Q_NO_DELTA" => out.q_no_delta = boolean()?,
                "TACTICAL_NO_HANG" => out.no_hang = boolean()?,
                "TACTICAL_NO_NULL" => out.no_null = boolean()?,
                "TACTICAL_NO_SOFT_STOP" => out.no_soft_stop = boolean()?,
                "TACTICAL_Q_DELTA_MARGIN" => {
                    out.q_delta_margin = value.parse().map_err(|_| format!("{key} must be a nonnegative i32"))?;
                    if out.q_delta_margin < 0 { return Err(format!("{key} must be nonnegative")); }
                }
                "TACTICAL_Q_TT" => out.q_tt = match value.as_str() {
                    "normal" => QTableMode::Normal, "hints" => QTableMode::Hints,
                    "context" => QTableMode::Context, _ => return Err(format!("unknown {key}={value}")),
                },
                "TACTICAL_LMR" => out.lmr = match value.as_str() {
                    "normal" => LmrMode::Normal, "evasions" => LmrMode::Evasions,
                    "off" => LmrMode::Off, "interior_off" => LmrMode::InteriorOff,
                    "root_off" => LmrMode::RootOff, _ => return Err(format!("unknown {key}={value}")),
                },
                "TACTICAL_STAND_PAT" => out.stand_pat = match value.as_str() {
                    "normal" => StandPatMode::Normal,
                    "major_capture_required" => StandPatMode::MajorCaptureRequired,
                    _ => return Err(format!("unknown {key}={value}")),
                },
                "TACTICAL_ROOT" => {
                    out.roots = value.split(';').map(str::to_owned).collect();
                    if out.roots.iter().any(|x| x.is_empty()) { return Err("TACTICAL_ROOT contains an empty route".into()); }
                }
                // The evaluator owns this independently parsed control.
                "TACTICAL_TEMPO_PERCENT" => {
                    let n: u32 = value.parse().map_err(|_| format!("invalid {key}"))?;
                    if n > 100 { return Err(format!("{key} must be 0..100")); }
                }
                _ => return Err(format!("unknown experimental variable {key}")),
            }
        }
        Ok(out)
    }

    pub(super) fn root_lmr(&self, checked: bool) -> bool {
        !matches!(self.lmr, LmrMode::Off | LmrMode::RootOff)
            && !(self.lmr == LmrMode::Evasions && checked)
    }
    pub(super) fn interior_lmr(&self, checked: bool) -> bool {
        !matches!(self.lmr, LmrMode::Off | LmrMode::InteriorOff)
            && !(self.lmr == LmrMode::Evasions && checked)
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct TacticalStats {
    pub q_evasion_nodes: u64,
    pub q_stand_pat_cutoffs: u64,
    pub q_depth_zero: u64,
    pub q_no_candidates: u64,
    pub q_tt_cutoffs: u64,
    pub q_delta_node_cuts: u64,
    pub q_delta_move_skips: u64,
    pub q_hang_skips: u64,
    pub ab_hang_skips: u64,
    pub root_reductions: u64,
    pub interior_reductions: u64,
    pub q_forced_capture_nodes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_is_strict_and_defaults_are_inert() {
        assert_eq!(TacticalOptions::parse(&HashMap::new()).unwrap(), TacticalOptions::default());
        for (k,v) in [("TACTICAL_NO_NULL","yes"),("TACTICAL_Q_DELTA_MARGIN","-1"),
            ("TACTICAL_Q_TT","typo"),("TACTICAL_ROOT",";"),("TACTICAL_TYPO","1"),
            ("TACTICAL_NO_SOFT_STOP","true")] {
            assert!(TacticalOptions::parse(&HashMap::from([(k.into(),v.into())])).is_err());
        }
        let o=TacticalOptions::parse(&HashMap::from([
            ("TACTICAL_Q_EVASIONS".into(),"1".into()),
            ("TACTICAL_NO_SOFT_STOP".into(),"1".into()),
            ("TACTICAL_LMR".into(),"evasions".into()),
            ("TACTICAL_Q_DELTA_MARGIN".into(),"500".into())])).unwrap();
        assert!(o.q_evasions && o.no_soft_stop && o.root_lmr(false) && !o.interior_lmr(true));
        assert!(!TacticalOptions::default().no_soft_stop);
        assert_eq!(o.q_delta_margin,500);
    }
}
