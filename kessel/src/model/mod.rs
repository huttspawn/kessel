//! Pure domain model: GOM payload bytes in, typed domain structs out.
//!
//! Nothing under `model/` touches SQLite or `Database`. Loading payloads and
//! writing rows is `crate::queries`; the spice.sqlite export and the graph
//! producer both call the same functions here.

pub mod ability;
pub mod discipline;
pub mod talent;

use std::collections::HashMap;

use crate::gsf_stat_dictionary::StatLabel;

/// One decoded stat record with its dictionary label and its rank: the
/// 1-based count of records sharing the same label within one object, in
/// payload order (a `+4/+8/+12` progression gets ranks 1, 2, 3).
#[derive(Debug, Clone, PartialEq)]
pub struct LabeledStat {
    /// The raw stat id (a `u8` for GSF talents, a `u16` prop id for GSF
    /// abilities).
    pub stat_id: u16,
    /// Position of the record in the payload.
    pub ordinal: u32,
    pub value: f32,
    pub rank: i64,
    pub label: String,
    pub unit: String,
    pub confidence: String,
}

/// Rank labelled records per label in the order given.
fn rank_by_label(
    records: impl IntoIterator<Item = (u16, u32, f32, StatLabel)>,
) -> Vec<LabeledStat> {
    let mut rank_per_label: HashMap<String, i64> = HashMap::new();
    records
        .into_iter()
        .map(|(stat_id, ordinal, value, label)| {
            let rank = rank_per_label.entry(label.label.clone()).or_insert(0);
            *rank += 1;
            LabeledStat {
                stat_id,
                ordinal,
                value,
                rank: *rank,
                label: label.label,
                unit: label.unit,
                confidence: label.confidence,
            }
        })
        .collect()
}
