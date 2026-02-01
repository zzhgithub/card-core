use crate::common::{CardInfoId, PlayerId};

/// 玩家信息
#[derive(Debug, Clone)]
pub struct Player {
    pub id: PlayerId,
    pub player_desk: PlayerDesk,
}

/// 玩家卡组信息
#[derive(Debug, Clone)]
pub struct PlayerDesk(pub Vec<CardInfoId>);
