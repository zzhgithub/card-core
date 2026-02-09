use crate::card::Card;
use crate::choice_req::ChoiceReq;
use crate::choice_res::ChoiceRes;
use crate::common::EntryId;
use crate::game::{Game, GamePhase};
use crate::targeting::Targeting;
use std::fmt::Debug;

pub trait ReadPlayerActions {
    // 在主要阶段读取数据
    fn read_action_main(&mut self);

    // 在战斗阶段读取数据
    fn read_action_fight(&mut self);
    fn read_fight_damage(&mut self) -> ChoiceRes;

    // 阅读选择指令
    fn read_choice(&mut self, choice: ChoiceReq) -> ChoiceRes;
    fn read_reuse_choice(&mut self, targeting: Targeting, limit:usize) -> Vec<EntryId>;

    fn help_main(&self);
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
    // 攻击
    AttackCard {
        source: Targeting,
        target: Targeting,
    },
    // 跳过
    Pass,
}
