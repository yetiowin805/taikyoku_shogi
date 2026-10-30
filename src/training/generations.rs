//! Retirement cohorts, independent of evaluator width and fitted rating.
use super::tournament::TourneyEntrant;

/// Explicit metadata supports future cohorts without renaming an agent. Old
/// state/manifest files keep working through the original roster's ID convention.
pub fn label(entrant: &TourneyEntrant) -> Result<Option<&str>, String> {
    if let Some(label) = entrant.generation.as_deref() {
        if label.is_empty() || label.trim() != label {
            return Err(format!("invalid generation label for {}", entrant.id));
        }
        return Ok(Some(label));
    }
    Ok(legacy_label(&entrant.id))
}

fn legacy_label(id: &str) -> Option<&'static str> {
    if id.starts_with("BASE_C2S2_")
        || matches!(
            id,
            "AVG_P120_SEED_C2R" | "C2K100A1D50R" | "T150_P120_T12_C2R" | "C2K50A1" | "SEEDS2"
        )
    {
        return Some("handcrafted");
    }
    let (width, version) = id.strip_prefix("NNUE_W")?.split_once('_')?;
    if width.is_empty() || !width.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // Search cohorts take precedence over the underlying network generation.
    // In particular both W384 HLF transfers belong to the second wave.
    if let Some((base, policy)) = version.split_once('_') {
        return match (base, policy) {
            ("v3", "H" | "L" | "N" | "S" | "T" | "Q" | "HL" | "ST" | "QFIX") => {
                Some("search_wave_1")
            }
            ("v3", "HLF" | "HLFN" | "HLF_CH" | "HLF_HR" | "HLF_QR" | "HLFS" | "HLFN_V" | "FN")
            | ("v4.5", "HLF") => Some("search_wave_2"),
            _ => None,
        };
    }
    match version {
        "v2" => Some("v2"),
        "v3" => Some("v3"),
        "v4" | "v4.5" => Some("v4_v4.5"),
        "v5a" | "v5b" | "v5c" => Some("v5"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_cohorts_and_search_precedence() {
        for (id, expected) in [
            ("BASE_C2S2_A0_Ldefense", "handcrafted"),
            ("SEEDS2", "handcrafted"),
            ("NNUE_W512_v2", "v2"),
            ("NNUE_W512_v3", "v3"),
            ("NNUE_W512_v4", "v4_v4.5"),
            ("NNUE_W384_v4.5", "v4_v4.5"),
            ("NNUE_W512_v5b", "v5"),
            ("NNUE_W512_v3_QFIX", "search_wave_1"),
            ("NNUE_W512_v3_HL", "search_wave_1"),
            ("NNUE_W512_v3_HLFN_V", "search_wave_2"),
            ("NNUE_W384_v3_HLF", "search_wave_2"),
            ("NNUE_W384_v4.5_HLF", "search_wave_2"),
        ] {
            assert_eq!(legacy_label(id), Some(expected), "{id}");
        }
        for id in [
            "unknown",
            "NNUE_W512_v3_UNKNOWN",
            "NNUE_W512_v6",
            "NNUE_Wx_v3",
        ] {
            assert_eq!(legacy_label(id), None, "{id}");
        }
    }

    #[test]
    fn explicit_cohorts_override_legacy_and_validate() {
        let mut entrant: TourneyEntrant = serde_json::from_str(
            r#"{"id":"NNUE_W512_v3_HLF","model":"unused","generation":"future_wave"}"#,
        )
        .unwrap();
        assert_eq!(label(&entrant).unwrap(), Some("future_wave"));
        for invalid in ["", " v3", "v3 "] {
            entrant.generation = Some(invalid.into());
            assert!(label(&entrant).is_err());
        }
        entrant.generation = None;
        assert_eq!(label(&entrant).unwrap(), Some("search_wave_2"));
        assert!(!serde_json::to_value(&entrant)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("generation"));
    }
}
