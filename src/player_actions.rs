use crate::card::Card;
use crate::choice_req::ChoiceReq;
use crate::choice_res::ChoiceRes;
use crate::common::EntryId;
use crate::game::{Game, GamePhase};
use crate::targeting::Targeting;
use std::fmt::Debug;

pub trait ReadPlayerActions {
    // 阅读指令
    fn read_action(&mut self, game_phase: GamePhase);

    // 阅读选择指令
    fn read_choice(&mut self, choice: ChoiceReq) -> ChoiceRes;

    fn help(&self);
}

#[derive(Debug, Clone)]
pub enum PlayerAction {
    // 放置卡片
    SetCard {
        card_id: EntryId,
        zone_id: EntryId,
    },
    // 发动效果
    EffectCard {
        card_id: EntryId,
    },
    // 选择卡片
    Targeting {
        target: Targeting,
    },
    // 攻击
    AttackCard {
        source: Targeting,
        target: Targeting,
    },
    // 跳过
    Pass,
}
