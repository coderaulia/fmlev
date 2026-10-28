//! Read-only preferred-moves (PPM) decoding. The native field is a 64-bit bitset at
//! player record + 0x200 (ACTUAL_PLAYER + 0x348). Only bits with screen evidence are named;
//! every other set bit is reported unnamed. There is no write support.
use serde::{Deserialize, Serialize};

pub const PPM_BITSET_OFFSET: usize = 0x200;

/// Bits proven by on-screen profiles. Bit 39 is set only for Mbappe (Cuts Inside From Both
/// Wings) and clear for Yamal and Haaland. Bits 35/51 and 11/14/58 are narrowed to move sets but
/// not ordered, so they are deliberately absent.
pub const VERIFIED_PPM_BITS: &[(u8, &str)] = &[(39, "Cuts Inside From Both Wings")];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreferredMoveBit {
    pub bit: u8,
    pub name: Option<String>,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreferredMoves {
    pub bitset: String,
    pub bits: Vec<PreferredMoveBit>,
    pub named_moves: Vec<String>,
    pub unnamed_set_bits: u32,
    pub status: String,
    pub write_supported: bool,
}

impl PreferredMoves {
    pub fn decode(raw: u64) -> Self {
        let bits: Vec<PreferredMoveBit> = (0u8..64)
            .filter(|b| raw >> b & 1 == 1)
            .map(|bit| {
                let name = VERIFIED_PPM_BITS.iter().find(|(b, _)| *b == bit).map(|(_, n)| n.to_string());
                PreferredMoveBit { bit, verified: name.is_some(), name }
            })
            .collect();
        let named_moves = bits.iter().filter_map(|b| b.name.clone()).collect::<Vec<_>>();
        let unnamed_set_bits = (bits.len() - named_moves.len()) as u32;
        Self {
            bitset: format!("0x{raw:x}"),
            bits,
            named_moves,
            unnamed_set_bits,
            status: "partial_bit_table".to_string(),
            write_supported: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_verified_bits_are_named() {
        let ppm = PreferredMoves::decode((1 << 39) | (1 << 35) | (1 << 3));
        assert_eq!(ppm.bitset, "0x8800000008");
        assert_eq!(ppm.bits.iter().map(|b| b.bit).collect::<Vec<_>>(), vec![3, 35, 39]);
        assert_eq!(ppm.named_moves, vec!["Cuts Inside From Both Wings"]);
        assert_eq!(ppm.unnamed_set_bits, 2);
        assert!(ppm.bits[2].verified && !ppm.bits[0].verified && ppm.bits[0].name.is_none());
        assert!(!ppm.write_supported);
    }

    #[test]
    fn player_payload_exposes_moves_only_when_read() {
        let mut player = crate::Player::default();
        let absent = serde_json::to_value(&player).unwrap();
        assert!(absent.get("preferred_moves").is_none());
        player.preferred_moves = Some(PreferredMoves::decode(1 << 39));
        let json = serde_json::to_value(&player).unwrap();
        assert_eq!(json["preferred_moves"]["bitset"], "0x8000000000");
        assert_eq!(json["preferred_moves"]["named_moves"][0], "Cuts Inside From Both Wings");
        assert_eq!(json["preferred_moves"]["write_supported"], false);
        let back: crate::Player = serde_json::from_value(json).unwrap();
        assert_eq!(back, player);
    }

    #[test]
    fn empty_and_full_bitsets_decode() {
        let none = PreferredMoves::decode(0);
        assert!(none.bits.is_empty() && none.named_moves.is_empty() && none.bitset == "0x0");
        let all = PreferredMoves::decode(u64::MAX);
        assert_eq!((all.bits.len(), all.unnamed_set_bits), (64, 63));
    }
}
