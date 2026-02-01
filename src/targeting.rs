use crate::common::EntryId;
use crate::effect::Condition;

/// 目标
#[derive(Debug, Clone, Default)]
pub enum Targeting {
    #[default]
    None,
    // 玩家自己
    TargetPlayerSelf,
    // 对手玩家
    TargetPlayerOpponent,
    // 卡片
    TargetCard(EntryId),
    // 场地
    TargetZone(EntryId),
}

#[derive(Debug, Clone, Default, Copy, Eq, PartialEq)]
pub enum Side {
    #[default]
    BothSide,
    PlayerSelf,
    PlayerOpponent,
}
