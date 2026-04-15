use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Side {
    Bid,
    Ask,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderUpdate {
    pub sequence: u64,
    pub instrument_id: u32,
    pub side: Side,
    pub price_ticks: u64,
    pub quantity: u32,
    pub event_ts_ns: u64,
}

impl OrderUpdate {
    #[must_use]
    pub fn synthetic(sequence: u64) -> Self {
        // Deterministic payload generation allows apples-to-apples comparison across benchmark modes.
        Self {
            sequence,
            instrument_id: ((sequence % 2048) + 1) as u32,
            side: if sequence.is_multiple_of(2) {
                Side::Bid
            } else {
                Side::Ask
            },
            price_ticks: 100_000 + (sequence % 10_000),
            quantity: ((sequence % 128) + 1) as u32,
            event_ts_ns: 1_700_000_000_000_000_000_u64.saturating_add(sequence),
        }
    }
}
