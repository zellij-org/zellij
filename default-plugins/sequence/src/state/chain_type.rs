use serde::{Deserialize, Serialize};
use zellij_tile::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChainType {
    And,
    Or,
    Then,
    None,
}

impl Default for ChainType {
    fn default() -> Self {
        ChainType::None
    }
}

impl ChainType {
    pub fn to_unblock_condition(&self) -> Option<UnblockCondition> {
        match self {
            ChainType::And => Some(UnblockCondition::OnExitSuccess),
            ChainType::Or => Some(UnblockCondition::OnExitFailure),
            ChainType::Then => Some(UnblockCondition::OnAnyExit),
            ChainType::None => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ChainType::And => "&&",
            ChainType::Or => "||",
            ChainType::Then => ";",
            ChainType::None => "",
        }
    }

    pub fn cycle_next(&mut self) {
        *self = match self {
            ChainType::And => ChainType::Or,
            ChainType::Or => ChainType::Then,
            ChainType::Then => ChainType::And,
            ChainType::None => ChainType::And,
        };
    }
}
