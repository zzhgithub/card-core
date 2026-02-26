use crate::choice_req::ChoiceReq;
use crate::choice_res::ChoiceRes;
use crate::common::EntryId;
use crate::game::{Game, GamePhase};
use crate::player_actions::PlayerAction;
use crate::targeting::Targeting;
use crate::targeting::Targeting::TargetZone;
use log::info;

impl Game {
    pub fn ai_read_action_main(&mut self) {
        while self.current_phase() == GamePhase::Main || self.current_phase() == GamePhase::Main2 {
            let player_id = self.current_player();
            info!("AI[{}] 开始主要阶段", player_id);

            let hand = self.current_hand();
            let mut set_actions = Vec::new();

            for &card_id in &hand {
                let card = self.get(card_id);
                if self.check_cost(card_id) {
                    let zones = self.current_zone();
                    for zone in &zones {
                        if let crate::game::Zone::FrontEnd { id, cards } = zone {
                            if cards.is_empty() {
                                set_actions.push((card_id, *id, card.card_info.cost));
                                break;
                            }
                        }
                    }
                }
            }

            set_actions.sort_by_key(|x| x.2);

            if set_actions.is_empty() {
                info!("AI[{}] 费用不足或无空区域，pass", player_id);
                break;
            }

            for (card_id, zone_id, cost) in set_actions {
                info!(
                    "AI[{}] 尝试登场卡片 {} 到区域 {}，费用 {}",
                    player_id, card_id, zone_id, cost
                );
                let action = PlayerAction::SetCard { card_id, zone_id };
                self.deal_player_action(action);
            }

            break;
        }
    }

    pub fn ai_read_action_fight(&mut self) {
        while self.current_phase() == GamePhase::Fight {
            let player_id = self.current_player();
            info!("AI[{}] 开始战斗阶段", player_id);

            let attack_zones = self.get_attack_zones();
            let attacked_zones = self.get_attacked_zones();

            if attack_zones.is_empty() {
                info!("AI[{}] 无可攻击区域，pass", player_id);
                break;
            }

            for attack_zone in &attack_zones {
                if let crate::game::Zone::FrontEnd {
                    id: atk_zone_id, ..
                } = attack_zone
                {
                    let has_front_end_target = attacked_zones.iter().any(|zone| {
                        if let crate::game::Zone::FrontEnd { cards, .. } = zone {
                            !cards.is_empty()
                        } else {
                            false
                        }
                    });

                    if has_front_end_target {
                        for target_zone in &attacked_zones {
                            if let crate::game::Zone::FrontEnd {
                                id: target_zone_id,
                                cards,
                            } = target_zone
                            {
                                if !cards.is_empty() {
                                    info!(
                                        "AI[{}] 攻击区域 {} -> {}",
                                        player_id, atk_zone_id, target_zone_id
                                    );
                                    self.deal_player_action(PlayerAction::AttackCard {
                                        source: TargetZone(*atk_zone_id),
                                        target: TargetZone(*target_zone_id),
                                    });
                                    break;
                                }
                            }
                        }
                    } else {
                        info!("AI[{}] 直接攻击对手", player_id);
                        self.deal_player_action(PlayerAction::AttackCard {
                            source: TargetZone(*atk_zone_id),
                            target: Targeting::TargetPlayerOpponent,
                        });
                    }
                }
            }

            break;
        }
    }

    pub fn ai_read_fight_damage(&mut self) -> ChoiceRes {
        let player_id = self.current_player();
        let real_point = self.current_real_point();

        if real_point > 0 {
            info!(
                "AI[{}] 消耗所有RealPoint造成伤害，RealPoint={}",
                player_id, real_point
            );
            return ChoiceRes::FightDamageByRealPoint(real_point);
        }

        info!("AI[{}] 放弃伤害，获得RealPoint", player_id);
        ChoiceRes::None
    }

    pub fn ai_read_choice(&mut self, choice: ChoiceReq) -> ChoiceRes {
        let player_id = self.current_player();
        match choice {
            ChoiceReq::Cost(card_id) => {
                if !self.check_cost(card_id) {
                    info!("AI[{}] 无法支付费用，取消操作", player_id);
                    self.set_rollback(card_id);
                    return ChoiceRes::None;
                }

                let card = self.get(card_id);
                let cost = card.card_info.cost;
                let hand = self.current_hand();
                let real_point = self.current_real_point();
                let current_cost_len = self.current_cost().len();
                // Cost区剩余可用槽位
                let available_cost_slots = 6usize.saturating_sub(current_cost_len);

                let mut use_hand: Vec<EntryId> = Vec::new();
                let mut remaining_cost = cost;

                // 只有当Cost区有空位时才能用手卡支付
                if available_cost_slots > 0 {
                    for &h in &hand {
                        if h != card_id
                            && remaining_cost > 0
                            && use_hand.len() < available_cost_slots
                        {
                            use_hand.push(h);
                            remaining_cost -= 1;
                        }
                    }
                }

                // 剩余费用用RealPoint支付
                let use_real_point = if remaining_cost > 0 {
                    remaining_cost.min(real_point)
                } else {
                    0
                };

                info!(
                    "AI[{}] 支付费用: 手牌 {:?}, RealPoint {}",
                    player_id, use_hand, use_real_point
                );

                ChoiceRes::Cost {
                    hands: use_hand,
                    real_point: use_real_point,
                }
            }
        }
    }

    pub fn ai_read_reuse_choice(&mut self, targeting: Targeting, limit: usize) -> Vec<EntryId> {
        let player_id = self.current_player();
        let costs = if let Targeting::TargetPlayerSelf = targeting {
            self.current_cost()
        } else {
            self.next_cost()
        };

        let take = costs.len().min(limit);
        let result: Vec<EntryId> = costs.iter().take(take).cloned().collect();

        info!("AI[{}] 回收卡片 {:?} (限制 {})", player_id, result, limit);

        result
    }
}
