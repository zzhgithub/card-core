use crate::card::Card;
use crate::choice_req::ChoiceReq;
use crate::choice_res::ChoiceRes;
use crate::common::{EntryId, IdGenerator, PlayerId};
use crate::effect::{Action, DoEffect, WindowsTag};
use crate::game_diff::GameDiff;
use crate::lua_api::LuaApi;
use crate::player::Player;
use crate::player_actions::{PlayerAction, ReadPlayerActions};
use crate::targeting::Targeting;
use crate::window_event::WindowEvent;
use log::{debug, error, info, trace, warn};
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::cmp::PartialEq;
use std::collections::{HashSet, VecDeque};

const MAX_HAND_SIZE: usize = 20;
const MAX_COST_SIZE: usize = 6;
const MAX_REAL_POINT: usize = 6;

// 游戏对象
#[derive(Clone, Debug)]
pub struct Game {
    // 两个玩家
    players: Vec<Player>,
    // 当前游戏阶段
    current_phase: GamePhase,
    // 当前玩家
    current_player: PlayerId,
    // 全部card实体
    cards: Vec<Card>,
    // 游戏内状态
    game_states: Vec<GameState>,
    // ID生成器
    id_generator: IdGenerator,
    // 操作的Effect列表
    do_effect_stacks: VecDeque<DoEffect>,
    // 游戏变化
    game_diff_list: Vec<GameDiff>,
    // 游戏结束
    game_over: Option<(PlayerId, GameOverReason)>,
    // AI玩家集合
    ai_players: HashSet<PlayerId>,
}

impl Game {
    // 创建游戏
    pub fn new(players: Vec<Player>, lua_api: &LuaApi, ai_players: HashSet<PlayerId>) -> Self {
        let mut id_generator = IdGenerator::new();
        let mut cards_all = Vec::new();
        let mut games_states = Vec::new();
        // 初始化卡组
        for player in players.iter() {
            let cards = Card::init(player.player_desk.clone(), lua_api, &mut id_generator);
            let mut game_state =
                GameState::new(player.id.clone(), cards.clone(), &mut id_generator);
            // 洗牌
            game_state.shuffle();
            // 抽卡
            game_state.draw(5);
            cards_all.extend(cards);
            games_states.push(game_state);
        }
        Self {
            players: players.clone(),
            current_phase: GamePhase::Start,
            current_player: 0,
            cards: cards_all,
            game_states: games_states,
            id_generator,
            do_effect_stacks: VecDeque::new(),
            game_diff_list: Vec::new(),
            game_over: None,
            ai_players,
        }
    }

    pub fn is_ai_player(&self, player_id: PlayerId) -> bool {
        self.ai_players.contains(&player_id)
    }

    pub fn get(&self, id: EntryId) -> Card {
        self.cards
            .iter()
            .filter(|card| card.entry_id == id)
            .next()
            .unwrap()
            .clone()
    }

    // 返回卡片的引用
    pub fn get_mut(&mut self, id: EntryId) -> &mut Card {
        self.cards
            .iter_mut()
            .filter(|card| card.entry_id == id)
            .next()
            .unwrap()
    }

    pub fn current_player(&self) -> PlayerId {
        self.current_player
    }

    pub fn current_grave(&self) -> Vec<EntryId> {
        self.game_states[self.current_player].grave.clone()
    }

    pub fn current_desk_len(&self) -> usize {
        self.game_states[self.current_player].desk.len()
    }

    pub fn current_zone(&self) -> Vec<Zone> {
        self.game_states[self.current_player].zone.clone()
    }

    pub fn current_hand(&self) -> Vec<EntryId> {
        self.game_states[self.current_player].hand.clone()
    }

    pub fn current_real_point(&self) -> usize {
        self.game_states[self.current_player].real_point
    }
    pub fn current_hp(&self) -> usize {
        self.game_states[self.current_player].hp
    }

    pub fn current_cost(&self) -> Vec<EntryId> {
        self.game_states[self.current_player].cost.clone()
    }

    pub fn next_cost(&self) -> Vec<EntryId> {
        let next_player = self.next_player_id();
        self.game_states[next_player].cost.clone()
    }

    // 检查 卡片当前费用是否足够
    pub fn check_cost(&self, card_id: EntryId) -> bool {
        // Cost区满时只能用RealPoint支付
        let available_cost_slots = MAX_COST_SIZE.saturating_sub(self.current_cost().len());
        let can_pay_by_hand = if available_cost_slots > 0 {
            self.game_states[self.current_player]
                .hand
                .len()
                .min(available_cost_slots)
        } else {
            0
        };
        let can_cost = can_pay_by_hand + self.current_real_point();
        let card = self.get(card_id);
        can_cost >= card.card_info.cost
    }

    // 登场支付费用回滚
    pub fn set_rollback(&mut self, card: EntryId) {
        if !self.game_states[self.current_player].cost.contains(&card) {
            // 只有从手卡区登场的卡才在这里返回
            self.game_states[self.current_player].hand.push(card);
        }
    }

    // 进入下一个阶段
    fn next_phase(&mut self) {
        self.current_phase = GamePhase::next(&self.current_phase)
    }

    // hands 支付的手牌 real_point 支付的点数
    pub fn cost(&mut self, hands: Vec<EntryId>, real_point: usize) {
        // Cost区满时不能用手卡支付
        let available_slots =
            MAX_COST_SIZE.saturating_sub(self.game_states[self.current_player].cost.len());

        if available_slots == 0 && !hands.is_empty() {
            warn!("Cost区已满，无法用手卡支付费用");
            // 将手卡返回
            for card in hands {
                self.game_states[self.current_player].hand.push(card);
            }
        } else {
            // 批量移动手卡到Cost区
            self.game_states[self.current_player]
                .hand
                .retain(|x| !hands.contains(x));

            let take_count = hands.len().min(available_slots);
            for card in hands.into_iter().take(take_count) {
                self.game_states[self.current_player].cost.push(card);
            }
        }
        // 减少RealPoint
        self.game_states[self.current_player].real_point -= real_point;
    }

    fn check_attack_action(&self, player_action: PlayerAction) -> bool {
        if let PlayerAction::AttackCard { source, target } = player_action {
            if let Targeting::TargetZone(zone_id) = source {
                if let Some(for_zone) = self.game_states[self.current_player]
                    .zone
                    .iter()
                    .filter(|zone| zone.has_id(zone_id) && zone.has_cards())
                    .next()
                {
                    return if self.zone_can_attack(for_zone) {
                        let vec = self.get_attacked_zones();
                        if vec.len() > 0 {
                            // 存在攻击区域
                            if let Targeting::TargetZone(target_id) = target {
                                true
                            } else {
                                error!("进攻目标不正确");
                                false
                            }
                        } else {
                            // 直接攻击玩家
                            if let Targeting::TargetPlayerOpponent = target {
                                true
                            } else {
                                error!("进攻目标不正确");
                                false
                            }
                        }
                    } else {
                        error!("当前区域不能攻击");
                        false
                    };
                } else {
                    error!("来源位置不存在");
                    return false;
                }
            } else {
                error!("攻击的卡选择的类型不正确");
                return false;
            }
        }
        false
    }

    pub fn deal_player_action(&mut self, player_acton: PlayerAction) {
        match player_acton.clone() {
            // 放置卡片
            PlayerAction::SetCard { card_id, zone_id } => {
                self.game_states[self.current_player]
                    .hand
                    .retain(|&x| x != card_id);

                let card = self.get(card_id);

                // 支持不支付费用的登场
                let choice_res = if card.card_info.cost > 0 {
                    if self.is_ai_player(self.current_player) {
                        self.ai_read_choice(ChoiceReq::Cost(card_id))
                    } else {
                        self.read_choice(ChoiceReq::Cost(card_id))
                    }
                } else {
                    ChoiceRes::Cost {
                        hands: Vec::new(),
                        real_point: 0,
                    }
                };

                if let ChoiceRes::Cost { hands, real_point } = choice_res {
                    self.cost(hands, real_point);
                    self.do_effect_stacks.push_front(DoEffect::Action {
                        source: Targeting::TargetPlayerSelf,
                        targeting: Targeting::None,
                        action: Action::Set { card_id, zone_id },
                    });
                    self.process_effect();
                }
            }
            PlayerAction::EffectCard { .. } => {}
            PlayerAction::AttackCard { source, target } => {
                // 判断源头是否合法
                // 判断对象是否合法
                if self.check_attack_action(player_acton.clone()) {
                    // 抛出事件
                    self.emit_event(WindowEvent::Attack {
                        source: source.clone(),
                        target: target.clone(),
                    });
                    // 处理战斗
                    self.deal_fight(source, target);
                    self.process_effect();
                }
            }
            PlayerAction::Pass => {}
        }
    }

    // 处理战斗
    pub fn deal_fight(&mut self, source: Targeting, target: Targeting) {
        // 如果源不存在了 就停止
        // 如果目标不存在了 或者有了新的目标 就回滚战斗需要询问对手
        // 进入战斗阶段
        if let Targeting::TargetZone(zone_id) = source {
            if let Some(for_zone) = self.game_states[self.current_player]
                .zone
                .iter()
                .filter(|zone| zone.has_id(zone_id) && zone.has_cards())
                .next()
            {
                let attacked_zones = self.get_attacked_zones();

                // 卡片和卡片进行战斗
                if let Targeting::TargetZone(target_id) = target {
                    // 判断 这个zoneId下是不是没有卡了
                    if let Some(_) = attacked_zones
                        .iter()
                        .filter(|zone| zone.has_id(target_id))
                        .next()
                    {
                        // 找到了进行结算
                        self.deal_fight_zone(zone_id, target_id);
                    } else {
                        // 没有找到进行询问
                    }
                }

                // 卡片直接攻击玩家
                if let Targeting::TargetPlayerOpponent = target {
                    // 判断这 zoneId是不是还有卡
                    if attacked_zones.len() > 0 {
                        // 进行询问
                    } else {
                        // 进行结算
                        self.deal_fight_direct(zone_id);
                    }
                }
            }
        }
    }

    // 处理直接攻击的情况
    fn deal_fight_direct(&mut self, my_zone: EntryId) {
        // 获取卡片的信息
        // effect 增加攻击计数

        if let Some(zone) = self.get_my_zone(my_zone) {
            match zone {
                Zone::FrontEnd { id, cards } => {
                    if let Some(card_id) = cards.first() {
                        // 攻击计数+1
                        self.do_effect_stacks.push_front(DoEffect::Action {
                            source: Targeting::None,
                            targeting: Targeting::None,
                            action: Action::AttackCounterUp(card_id.clone(), 1),
                        });

                        if self.current_real_point() == 0 {
                            // 如果 realPoint = 0 realPoint +1
                            info!("当前无RealPoint。");
                            self.do_effect_stacks.push_front(DoEffect::Action {
                                source: Default::default(),
                                targeting: Targeting::TargetPlayerSelf,
                                action: Action::AddRealPoint(1),
                            });
                        } else {
                            // 如果 realPoint > 0 询问 是否要使用 如果使用了 则 伤害 扣除 RealPoint
                            info!("当前有RealPoint。询问如何使用");
                            let choice_res = self.read_fight_damage();
                            match choice_res {
                                ChoiceRes::None => {
                                    self.do_effect_stacks.push_front(DoEffect::Action {
                                        source: Default::default(),
                                        targeting: Targeting::TargetPlayerSelf,
                                        action: Action::AddRealPoint(1),
                                    });
                                }
                                ChoiceRes::FightDamageByRealPoint(num) => {
                                    // 使用RealPoint
                                    self.do_effect_stacks.push_front(DoEffect::Action {
                                        source: Default::default(),
                                        targeting: Targeting::TargetPlayerSelf,
                                        action: Action::UseRealPoint(num),
                                    });
                                    // 造成伤害
                                    self.do_effect_stacks.push_front(DoEffect::Action {
                                        source: Targeting::TargetZone(my_zone.clone()),
                                        targeting: Targeting::TargetPlayerOpponent,
                                        action: Action::Damage(num),
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Zone::BackEnd { .. } => {
                    warn!("后场卡不战斗");
                }
            }
        }
    }

    // 计算的发生的战斗
    fn deal_fight_zone(&mut self, my_zone_id: EntryId, target_zone_id: EntryId) {
        if let Some(my_zone) = self.get_my_zone(my_zone_id) {
            if let Some(target_zone) = self.get_other_zone(target_zone_id) {
                if let Zone::FrontEnd {
                    id: _id,
                    cards: my_cards,
                } = my_zone
                {
                    if let Zone::FrontEnd {
                        id: _id,
                        cards: target_cards,
                    } = target_zone
                    {
                        let card = self.get(my_cards.first().unwrap().clone());
                        let target_card = self.get(target_cards.first().unwrap().clone());
                        if card.card_info.ack > target_card.card_info.ack {
                            // 攻击胜利
                            info!(
                                "战斗胜利 {:?} > {:?}",
                                card.card_info.ack, target_card.card_info.ack
                            );
                            // 攻击计数+1
                            self.do_effect_stacks.push_front(DoEffect::Action {
                                source: Targeting::None,
                                targeting: Targeting::None,
                                action: Action::AttackCounterUp(card.entry_id, 1),
                            });
                            // 破坏对手卡
                            self.do_effect_stacks.push_front(DoEffect::Action {
                                source: Targeting::None,
                                targeting: Targeting::None,
                                action: Action::FightDestroy {
                                    zone_id: target_zone_id,
                                },
                            });
                            // 增加点数
                            self.do_effect_stacks.push_front(DoEffect::Action {
                                source: Targeting::None,
                                targeting: Targeting::TargetPlayerSelf,
                                action: Action::AddRealPoint(1),
                            });
                        } else if card.card_info.ack == target_card.card_info.ack {
                            info!(
                                "战斗平手 {:?} = {:?}",
                                card.card_info.ack, target_card.card_info.ack
                            );
                            // 平手
                            // 两张卡都破坏
                            self.do_effect_stacks.push_front(DoEffect::Action {
                                source: Targeting::None,
                                targeting: Targeting::None,
                                action: Action::FightDestroy {
                                    zone_id: target_zone_id,
                                },
                            });
                            self.do_effect_stacks.push_front(DoEffect::Action {
                                source: Targeting::None,
                                targeting: Targeting::None,
                                action: Action::FightDestroy {
                                    zone_id: my_zone_id,
                                },
                            });
                            // 增加点数
                            self.do_effect_stacks.push_front(DoEffect::Action {
                                source: Targeting::None,
                                targeting: Targeting::TargetPlayerSelf,
                                action: Action::AddRealPoint(1),
                            });
                        } else {
                            info!(
                                "战斗失败 {:?} = {:?}",
                                card.card_info.ack, target_card.card_info.ack
                            );
                            // 破坏自己卡
                            self.do_effect_stacks.push_front(DoEffect::Action {
                                source: Targeting::None,
                                targeting: Targeting::None,
                                action: Action::FightDestroy {
                                    zone_id: my_zone_id,
                                },
                            });
                            // 增加点数
                            self.do_effect_stacks.push_front(DoEffect::Action {
                                source: Targeting::None,
                                targeting: Targeting::TargetPlayerSelf,
                                action: Action::AddRealPoint(1),
                            });
                        }
                    }
                }
            }
        }
    }

    fn get_my_zone(&self, id: EntryId) -> Option<&Zone> {
        self.game_states[self.current_player]
            .zone
            .iter()
            .filter(|zone| zone.has_id(id))
            .next()
    }

    fn get_other_zone(&self, id: EntryId) -> Option<&Zone> {
        self.game_states[self.next_player_id()]
            .zone
            .iter()
            .filter(|zone| zone.has_id(id))
            .next()
    }

    fn remove_my_zone_cards(&mut self, id: EntryId) -> Vec<EntryId> {
        let mut ret = Vec::new();
        if let Some(zone) = self.game_states[self.current_player]
            .zone
            .iter_mut()
            .filter(|zone| zone.has_id(id))
            .next()
        {
            match zone {
                Zone::FrontEnd { id, cards } => {
                    ret.extend(cards.drain(..));
                }
                Zone::BackEnd { id, cards } => {
                    ret.extend(cards.drain(..));
                }
            }
        }
        ret
    }

    fn remove_other_zone_cards(&mut self, id: EntryId) -> Vec<EntryId> {
        let mut ret = Vec::new();
        let next_id = self.next_player_id();
        if let Some(zone) = self.game_states[next_id]
            .zone
            .iter_mut()
            .filter(|zone| zone.has_id(id))
            .next()
        {
            match zone {
                Zone::FrontEnd { id, cards } => {
                    ret.extend(cards.drain(..));
                }
                Zone::BackEnd { id, cards } => {
                    ret.extend(cards.drain(..));
                }
            }
        }
        ret
    }

    pub fn emit_event(&mut self, window_event: WindowEvent) {
        match window_event {
            WindowEvent::Cost { card } => {
                // 这里实现登场时效果事件 这里要有一个自排连锁的问题
                warn!("TODO");
            }
            WindowEvent::Set { card } => {
                let card_instance = self.get(card);
                for effect in card_instance.card_info.effects {
                    if effect.windows_tag == WindowsTag::OnSet {
                        // todo 这里在思考更加复杂的 情况
                        info!("登场时发动效果：{:?}", effect.do_effect);
                        self.do_effect_stacks.push_front(effect.do_effect.clone());
                    }
                }
            }
            WindowEvent::Attack { source, target } => {
                // 处理攻击时的效果
                warn!("Attack Event TODO");
            }
        }
        self.process_effect();
    }

    // 结算效果
    pub fn process_effect(&mut self) {
        while let Some(event) = self.do_effect_stacks.pop_front() {
            match event {
                DoEffect::None => {
                    warn!("Do effect DoNothing!");
                }
                DoEffect::Action {
                    source,
                    targeting,
                    action,
                } => match action {
                    Action::None => {
                        debug!("Do effect DoNothing!");
                    }
                    Action::Draw(num) => match targeting {
                        Targeting::TargetPlayerSelf => {
                            let deck_out = self.game_states[self.current_player].draw(num);
                            if deck_out {
                                self.game_over =
                                    Some((self.current_player, GameOverReason::DeckOut));
                                return;
                            }
                        }
                        Targeting::TargetPlayerOpponent => {
                            let i = self.next_player_id();
                            let deck_out = self.game_states[i].draw(num);
                            if deck_out {
                                self.game_over = Some((i, GameOverReason::DeckOut));
                                return;
                            }
                        }
                        _ => {}
                    },
                    Action::Set { card_id, zone_id } => {
                        // 登场卡片
                        info!("卡片登场!Card {:?} 登场到 Zone {:?}", card_id, zone_id);
                        // 原来的位置 要取消数据
                        // 如果Cost 里面有的情况要取消
                        self.game_states[self.current_player]
                            .cost
                            .retain(|&x| x != card_id);
                        let mut flag = false;
                        for zone in self.game_states[self.current_player.clone()]
                            .zone
                            .iter_mut()
                        {
                            match zone {
                                Zone::FrontEnd { id, cards } => {
                                    if *id == zone_id {
                                        cards.push(card_id);
                                        flag = true;
                                    }
                                }
                                Zone::BackEnd { id, cards } => {
                                    if *id == zone_id {
                                        cards.push(card_id);
                                    }
                                }
                            }
                        }
                        // 抛出登场时事件
                        if flag {
                            self.emit_event(WindowEvent::Set { card: card_id });
                        }
                    }
                    Action::Damage(num) => {
                        if let Targeting::TargetPlayerSelf = targeting {
                            let real_point = self.game_states[self.current_player].real_point;
                            let hp = self.game_states[self.current_player].hp;
                            if hp <= num {
                                warn!("当前玩家生命值不足");
                                if real_point + hp > num {
                                    info!("使用RealPoint保护生命");
                                    self.game_states[self.current_player].hp = 1;
                                    self.game_states[self.current_player].real_point =
                                        real_point - (num - hp + 1);
                                } else {
                                    self.game_states[self.current_player].hp = 0;
                                    self.game_states[self.current_player].real_point = 0;
                                    self.game_over =
                                        Some((self.current_player, GameOverReason::HpZero));
                                    return;
                                }
                            } else {
                                self.game_states[self.current_player].hp -= num;
                                info!(
                                    "伤害后的生命值hp[{:?}]",
                                    self.game_states[self.current_player].hp
                                );
                            }
                        }
                        if let Targeting::TargetPlayerOpponent = targeting {
                            let next_id = self.next_player_id();
                            let real_point = self.game_states[next_id].real_point;
                            let hp = self.game_states[next_id].hp;
                            if hp <= num {
                                warn!("对方玩家生命值不足");
                                if real_point + hp > num {
                                    info!("使用RealPoint保护生命");
                                    self.game_states[next_id].hp = 1;
                                    self.game_states[next_id].real_point =
                                        real_point - (num - hp + 1);
                                } else {
                                    self.game_states[next_id].hp = 0;
                                    self.game_states[next_id].real_point = 0;
                                    self.game_over = Some((next_id, GameOverReason::HpZero));
                                    return;
                                }
                            } else {
                                self.game_states[next_id].hp -= num;
                                info!("伤害后的生命值hp[{:?}]", self.game_states[next_id].hp);
                            }
                        }
                    }
                    Action::AddRealPoint(num) => {
                        if let Targeting::TargetPlayerSelf = targeting {
                            let current = self.game_states[self.current_player].real_point;
                            if current >= MAX_REAL_POINT {
                                warn!("RealPoint已达到上限[{}]，不再增加", MAX_REAL_POINT);
                            } else {
                                let new_value = (current + num).min(MAX_REAL_POINT);
                                self.game_states[self.current_player].real_point = new_value;
                                info!("当前玩家RealPoint 增加[{:?}]，当前[{:?}]", num, new_value);
                            }
                        }
                        if let Targeting::TargetPlayerOpponent = targeting {
                            let next_id = self.next_player_id();
                            let current = self.game_states[next_id].real_point;
                            if current >= MAX_REAL_POINT {
                                warn!("RealPoint已达到上限[{}]，不再增加", MAX_REAL_POINT);
                            } else {
                                let new_value = (current + num).min(MAX_REAL_POINT);
                                self.game_states[next_id].real_point = new_value;
                                info!("对方玩家RealPoint 增加[{:?}]，当前[{:?}]", num, new_value);
                            }
                        }
                    }
                    Action::UseRealPoint(num) => {
                        if let Targeting::TargetPlayerSelf = targeting {
                            if self.game_states[self.current_player].real_point >= num {
                                self.game_states[self.current_player].real_point -= num;
                                info!("当前玩家RealPoint 减少[{:?}]", num);
                            }
                        }
                        if let Targeting::TargetPlayerOpponent = targeting {
                            let next_id = self.next_player_id();
                            if self.game_states[next_id].real_point >= num {
                                self.game_states[next_id].real_point -= num;
                                info!("对方玩家RealPoint 减少[{:?}]", num);
                            }
                        }
                    }
                    // 战斗破坏
                    Action::FightDestroy { zone_id } => {
                        let cards = self.remove_my_zone_cards(zone_id);
                        self.destroy_zone(cards, Targeting::TargetPlayerSelf);

                        let cards = self.remove_other_zone_cards(zone_id);
                        self.destroy_zone(cards, Targeting::TargetPlayerOpponent);
                    }
                    Action::AttackCounterUp(card_id, num) => {
                        let card = self.get_mut(card_id);
                        card.attack_counter += num;
                    }
                    Action::AttackCountDown(card_id, num) => {
                        let card = self.get_mut(card_id);
                        if card.attack_counter >= num {
                            card.attack_counter -= num;
                        } else {
                            card.attack_counter = 0;
                        }
                    }
                    Action::AskingReuse(limit) => {
                        // todo 向需要的人询问
                        let choice_cards = self.read_reuse_choice(targeting.clone(), limit);
                        self.do_effect_stacks.push_front(DoEffect::Action {
                            source: Default::default(),
                            targeting,
                            action: Action::Reuse(choice_cards),
                        });
                    }
                    Action::Reuse(cost_cards) => {
                        // 回收卡片进手卡
                        info!("回收卡片进手卡");
                        if let Targeting::TargetPlayerSelf = targeting {
                            // 自己的卡回收
                            self.game_states[self.current_player]
                                .cost
                                .retain(|x| !cost_cards.contains(x));
                            self.game_states[self.current_player]
                                .hand
                                .extend(cost_cards);
                        } else if let Targeting::TargetPlayerOpponent = targeting {
                            let next_id = self.next_player_id();
                            // 对手的卡回收
                            self.game_states[next_id]
                                .cost
                                .retain(|x| !cost_cards.contains(x));
                            self.game_states[next_id].hand.extend(cost_cards);
                        }
                    }
                },
                DoEffect::AndAction(actions) => {
                    for action in actions {
                        self.do_effect_stacks.push_back(action);
                    }
                }
                DoEffect::OrAction(_) => {
                    // todo 需要用户选择
                }
            }
        }
    }

    // 破坏场地上的卡
    pub fn destroy_zone<T: IntoIterator<Item = EntryId>>(
        &mut self,
        cards: T,
        targeting: Targeting,
    ) {
        let player_id = if let Targeting::TargetPlayerSelf = targeting {
            self.current_player.clone()
        } else {
            self.next_player_id()
        };
        self.game_states[player_id].grave.extend(cards);
    }

    pub fn current_phase(&self) -> GamePhase {
        self.current_phase
    }

    // 对手id
    pub fn next_player_id(&self) -> PlayerId {
        (self.current_player + 1) % self.players.len()
    }

    // 判断区域是否可以攻击
    pub fn zone_can_attack(&self, zone: &Zone) -> bool {
        if let Zone::FrontEnd { id, cards } = zone {
            if let Some(first) = cards.first() {
                if let card = self.get(first.clone()) {
                    return card.attack_counter < card.attack_max;
                }
            }
        }
        false
    }

    // 获取可以进攻的 区域
    pub fn get_attack_zones(&self) -> Vec<Zone> {
        self.game_states[self.current_player]
            .zone
            .iter()
            .filter(|&zone| self.zone_can_attack(zone))
            .cloned()
            .collect()
    }

    // 获取对手场上可以攻击的区域
    pub fn get_attacked_zones(&self) -> Vec<Zone> {
        self.game_states[self.next_player_id()]
            .zone
            .iter()
            .filter(|&zone| {
                if let Zone::FrontEnd { id, cards } = zone {
                    return cards.len() > 0;
                }
                false
            })
            .cloned()
            .collect()
    }

    // 刷新卡片上的计数器
    fn flash_cards(&mut self) {
        info!("刷新卡片的计数器");
        for zone in self.game_states[self.current_player].zone.clone().iter() {
            match zone {
                Zone::FrontEnd { id, cards } => {
                    for entry_id in cards.iter() {
                        let card = self.get_mut(entry_id.clone());
                        card.attack_counter = 0;
                    }
                }
                Zone::BackEnd { id, cards } => {
                    // todo 这里没有攻击次数但是 有其他引用计数
                }
            }
        }
    }

    // 获取对手场上费用最高的一张卡的费用
    fn get_highest_cost_other_zone(&self) -> usize {
        let next_id = self.next_player_id();
        let mut all_card_ids = Vec::new();
        for zone in &self.game_states[next_id].zone {
            if let Zone::FrontEnd { id, cards } = zone {
                if let Some(first) = cards.first() {
                    all_card_ids.push(first.clone());
                }
            }
        }

        all_card_ids
            .iter()
            .map(|id| self.get(id.clone()).card_info.cost)
            .max()
            .unwrap_or(0)
    }

    // 开始游戏
    pub fn run(&mut self) {
        loop {
            let player = &self.players[self.current_player];
            match self.current_phase {
                GamePhase::Start => {
                    info!("player[{:?}] 回合开始阶段", self.current_player);
                    self.flash_cards();
                    self.next_phase();
                }
                GamePhase::Draw => {
                    info!("player[{:?}] 抽卡阶段", self.current_player);
                    // todo 这里先实现简单 无事件版本的抽卡
                    self.do_effect_stacks.push_front(DoEffect::Action {
                        source: Default::default(),
                        targeting: Targeting::TargetPlayerSelf,
                        action: Action::Draw(1),
                    });
                    self.process_effect();
                    if self.game_over.is_some() {
                        break;
                    }
                    self.next_phase();
                }
                GamePhase::Reuse => {
                    info!("player[{:?}] 回收阶段", self.current_player);
                    let my_cost_len = self.current_cost().len();
                    info!("Cost区长度: {:?}", my_cost_len);
                    if my_cost_len > 0 {
                        let highest_cost = self.get_highest_cost_other_zone();
                        let reuse_count = if highest_cost > 0 {
                            highest_cost.min(my_cost_len)
                        } else {
                            my_cost_len
                        };
                        info!("回收 {} 张卡片", reuse_count);
                        let cost_cards: Vec<EntryId> = self
                            .current_cost()
                            .iter()
                            .take(reuse_count)
                            .cloned()
                            .collect();
                        self.do_effect_stacks.push_front(DoEffect::Action {
                            source: Default::default(),
                            targeting: Targeting::TargetPlayerSelf,
                            action: Action::Reuse(cost_cards),
                        });
                    }
                    self.process_effect();
                    self.next_phase();
                }
                GamePhase::Main => {
                    info!("player[{:?}] 主要阶段1", self.current_player);
                    if self.is_ai_player(self.current_player) {
                        self.ai_read_action_main();
                    } else {
                        self.help_main();
                        self.read_action_main();
                    }
                    if self.game_over.is_some() {
                        break;
                    }
                    self.next_phase();
                }
                GamePhase::Fight => {
                    info!("player[{:?}] 战斗阶段", self.current_player);
                    if self.is_ai_player(self.current_player) {
                        self.ai_read_action_fight();
                    } else {
                        self.read_action_fight();
                    }
                    if self.game_over.is_some() {
                        break;
                    }
                    self.next_phase();
                }
                GamePhase::Main2 => {
                    info!("player[{:?}] 主要阶段2", self.current_player);
                    if self.is_ai_player(self.current_player) {
                        self.ai_read_action_main();
                    } else {
                        self.help_main();
                        self.read_action_main();
                    }
                    if self.game_over.is_some() {
                        break;
                    }
                    self.next_phase();
                }
                GamePhase::End => {
                    info!("player[{:?}] 回合结束阶段", self.current_player);
                    self.next_phase();
                    self.current_player = self.next_player_id();
                }
            }
        }

        // 输出游戏结果
        if let Some((loser_id, reason)) = &self.game_over {
            let winner_id = (*loser_id + 1) % self.players.len();
            let reason_text = match reason {
                GameOverReason::DeckOut => "卡组耗尽，无法抽卡",
                GameOverReason::HpZero => "生命值归零",
            };
            info!("========== 游戏结束 ==========");
            info!("败者: 玩家[{}], 原因: {}", loser_id, reason_text);
            info!("胜者: 玩家[{}]", winner_id);
            info!("==============================");
        }
    }
}

/// 游戏阶段
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum GamePhase {
    // 回合开始阶段
    Start,
    // 抽卡阶段
    Draw,
    // 回收阶段
    Reuse,
    // 主要阶段
    Main,
    //战斗阶段
    Fight,
    // 主要阶段2
    Main2,
    // 结束阶段
    End,
}

impl GamePhase {
    pub fn next(phase: &GamePhase) -> GamePhase {
        match phase {
            GamePhase::Start => GamePhase::Draw,
            GamePhase::Draw => GamePhase::Reuse,
            GamePhase::Reuse => GamePhase::Main,
            GamePhase::Main => GamePhase::Fight,
            GamePhase::Fight => GamePhase::Main2,
            GamePhase::Main2 => GamePhase::End,
            GamePhase::End => GamePhase::Start,
        }
    }
}

/// 游戏结束原因
#[derive(Clone, Debug)]
pub enum GameOverReason {
    /// 卡组没有卡时抽卡
    DeckOut,
    /// 生命值小于等于0
    HpZero,
}

#[derive(Clone, Debug)]
pub struct GameState {
    player_id: PlayerId,
    // 生命值
    hp: usize,
    // 真实点数
    real_point: usize,
    // 卡组区域
    desk: Vec<EntryId>,
    // 手牌区域
    hand: Vec<EntryId>,
    // 费用区域
    cost: Vec<EntryId>,
    // 墓地区域
    grave: Vec<EntryId>,
    // 其他区域
    zone: Vec<Zone>,
}

impl GameState {
    fn new(player_id: PlayerId, cards: Vec<Card>, id_generator: &mut IdGenerator) -> Self {
        GameState {
            player_id,
            hp: 6,
            real_point: 0,
            desk: cards.iter().map(|cards| cards.entry_id).collect(),
            hand: Vec::new(),
            cost: Vec::new(),
            grave: Vec::new(),
            zone: Zone::init(id_generator),
        }
    }

    pub fn player_id(&self) -> PlayerId {
        self.player_id
    }

    // 洗牌算法
    pub fn shuffle(&mut self) {
        self.desk.shuffle(&mut thread_rng());
    }

    // 抽卡 返回true表示卡组耗尽
    pub fn draw(&mut self, num: usize) -> bool {
        for _ in 0..num {
            if let Some(entry_id) = self.desk.pop() {
                if self.hand.len() >= MAX_HAND_SIZE {
                    self.grave.push(entry_id);
                    warn!("手卡已满[{}]，卡片进入墓地", MAX_HAND_SIZE);
                } else {
                    self.hand.push(entry_id);
                }
            } else {
                warn!("卡组耗尽，无法抽卡");
                return true;
            }
        }
        false
    }
}

/// 场地
#[derive(Clone, Debug)]
pub enum Zone {
    //前场
    FrontEnd {
        id: EntryId,
        cards: Vec<EntryId>,
        //todo 属性
    },
    // 后场
    BackEnd {
        id: EntryId,
        cards: Vec<EntryId>,
    },
}

impl Zone {
    pub fn init(id_generator: &mut IdGenerator) -> Vec<Zone> {
        // 初始化一块场地信息
        let mut ret = Vec::new();
        // 前场5个 后场5个
        for _ in 0..4 {
            ret.push(Zone::FrontEnd {
                id: id_generator.next(),
                cards: Vec::new(),
            });
        }
        for _ in 0..4 {
            ret.push(Zone::BackEnd {
                id: id_generator.next(),
                cards: Vec::new(),
            });
        }
        ret
    }

    // 是否包含id
    pub fn has_id(&self, zone_id: EntryId) -> bool {
        match self {
            Zone::FrontEnd { id, cards } => id.clone() == zone_id,
            Zone::BackEnd { id, cards } => id.clone() == zone_id,
        }
    }

    // 是否有卡片
    pub fn has_cards(&self) -> bool {
        match self {
            Zone::FrontEnd { id, cards } => cards.len() > 0,
            Zone::BackEnd { id, cards } => cards.len() > 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::CardInfoBuilder;
    use crate::player::PlayerDesk;

    fn test_lua_api() -> LuaApi {
        let mut api = LuaApi::new();
        let card_info = CardInfoBuilder::new("test-card".to_string()).build();
        api.cards.insert("test-card".to_string(), card_info);
        api
    }

    fn test_game(desk_size: usize) -> Game {
        let desk = PlayerDesk(vec!["test-card".to_string(); desk_size]);
        let players = vec![
            Player {
                id: 0,
                player_desk: desk.clone(),
            },
            Player {
                id: 1,
                player_desk: desk.clone(),
            },
        ];
        let api = test_lua_api();
        Game::new(players, &api, HashSet::new())
    }

    fn test_card(id_gen: &mut IdGenerator) -> Card {
        Card {
            entry_id: id_gen.next(),
            card_info: CardInfoBuilder::new("test".to_string()).build(),
            attack_counter: 0,
            attack_max: 1,
        }
    }

    // === GameState::draw 单元测试 ===

    #[test]
    fn test_draw_success() {
        let mut id_gen = IdGenerator::new();
        let cards = vec![test_card(&mut id_gen)];
        let mut state = GameState::new(0, cards, &mut id_gen);
        assert!(!state.draw(1));
        assert_eq!(state.hand.len(), 1);
        assert_eq!(state.desk.len(), 0);
    }

    #[test]
    fn test_draw_empty_deck_returns_true() {
        let mut id_gen = IdGenerator::new();
        let mut state = GameState::new(0, vec![], &mut id_gen);
        assert!(state.draw(1));
        assert_eq!(state.hand.len(), 0);
    }

    #[test]
    fn test_draw_partial_deck_out() {
        let mut id_gen = IdGenerator::new();
        let cards = vec![test_card(&mut id_gen)];
        let mut state = GameState::new(0, cards, &mut id_gen);
        // 卡组只有1张，抽2张，抽到第2张时耗尽
        assert!(state.draw(2));
        assert_eq!(state.hand.len(), 1); // 成功抽到1张
    }

    // === 卡组耗尽 游戏结束判定 ===

    #[test]
    fn test_game_over_deck_out_current_player() {
        let mut game = test_game(10);
        let current = game.current_player;
        // 清空当前玩家卡组
        game.game_states[current].desk.clear();
        // 推入抽卡效果
        game.do_effect_stacks.push_front(DoEffect::Action {
            source: Targeting::None,
            targeting: Targeting::TargetPlayerSelf,
            action: Action::Draw(1),
        });
        game.process_effect();
        let (loser, reason) = game.game_over.clone().unwrap();
        assert_eq!(loser, current);
        assert!(matches!(reason, GameOverReason::DeckOut));
    }

    #[test]
    fn test_game_over_deck_out_opponent() {
        let mut game = test_game(10);
        let opponent = game.next_player_id();
        // 清空对手卡组
        game.game_states[opponent].desk.clear();
        // 推入对手抽卡效果
        game.do_effect_stacks.push_front(DoEffect::Action {
            source: Targeting::None,
            targeting: Targeting::TargetPlayerOpponent,
            action: Action::Draw(1),
        });
        game.process_effect();
        let (loser, reason) = game.game_over.clone().unwrap();
        assert_eq!(loser, opponent);
        assert!(matches!(reason, GameOverReason::DeckOut));
    }

    #[test]
    fn test_no_game_over_on_successful_draw() {
        let mut game = test_game(10); // 10张卡，初始抽5张，剩5张
        game.do_effect_stacks.push_front(DoEffect::Action {
            source: Targeting::None,
            targeting: Targeting::TargetPlayerSelf,
            action: Action::Draw(1),
        });
        game.process_effect();
        assert!(game.game_over.is_none());
    }

    // === 生命值归零 游戏结束判定 ===

    #[test]
    fn test_game_over_hp_zero_current_player() {
        let mut game = test_game(10);
        let current = game.current_player;
        game.game_states[current].hp = 3;
        game.game_states[current].real_point = 0;
        // 造成5点伤害，hp=3 不够，real_point=0 无法保护
        game.do_effect_stacks.push_front(DoEffect::Action {
            source: Targeting::None,
            targeting: Targeting::TargetPlayerSelf,
            action: Action::Damage(5),
        });
        game.process_effect();
        let (loser, reason) = game.game_over.clone().unwrap();
        assert_eq!(loser, current);
        assert!(matches!(reason, GameOverReason::HpZero));
        assert_eq!(game.game_states[current].hp, 0);
    }

    #[test]
    fn test_game_over_hp_zero_opponent() {
        let mut game = test_game(10);
        let opponent = game.next_player_id();
        game.game_states[opponent].hp = 2;
        game.game_states[opponent].real_point = 0;
        game.do_effect_stacks.push_front(DoEffect::Action {
            source: Targeting::None,
            targeting: Targeting::TargetPlayerOpponent,
            action: Action::Damage(5),
        });
        game.process_effect();
        let (loser, reason) = game.game_over.clone().unwrap();
        assert_eq!(loser, opponent);
        assert!(matches!(reason, GameOverReason::HpZero));
        assert_eq!(game.game_states[opponent].hp, 0);
    }

    #[test]
    fn test_game_over_hp_exact_zero() {
        let mut game = test_game(10);
        let current = game.current_player;
        game.game_states[current].hp = 3;
        game.game_states[current].real_point = 0;
        // 伤害恰好等于hp
        game.do_effect_stacks.push_front(DoEffect::Action {
            source: Targeting::None,
            targeting: Targeting::TargetPlayerSelf,
            action: Action::Damage(3),
        });
        game.process_effect();
        let (loser, reason) = game.game_over.clone().unwrap();
        assert_eq!(loser, current);
        assert!(matches!(reason, GameOverReason::HpZero));
    }

    // === RealPoint 保护生命 ===

    #[test]
    fn test_hp_protected_by_real_point() {
        let mut game = test_game(10);
        let current = game.current_player;
        game.game_states[current].hp = 2;
        game.game_states[current].real_point = 5;
        // 伤害3，hp(2) < 3，但 real_point(5)+hp(2) = 7 > 3，存活
        game.do_effect_stacks.push_front(DoEffect::Action {
            source: Targeting::None,
            targeting: Targeting::TargetPlayerSelf,
            action: Action::Damage(3),
        });
        game.process_effect();
        assert!(game.game_over.is_none());
        assert_eq!(game.game_states[current].hp, 1);
    }

    #[test]
    fn test_hp_not_protected_when_real_point_insufficient() {
        let mut game = test_game(10);
        let current = game.current_player;
        game.game_states[current].hp = 2;
        game.game_states[current].real_point = 1;
        // 伤害5，hp(2)+real_point(1) = 3 < 5，无法存活
        game.do_effect_stacks.push_front(DoEffect::Action {
            source: Targeting::None,
            targeting: Targeting::TargetPlayerSelf,
            action: Action::Damage(5),
        });
        game.process_effect();
        let (loser, reason) = game.game_over.clone().unwrap();
        assert_eq!(loser, current);
        assert!(matches!(reason, GameOverReason::HpZero));
        assert_eq!(game.game_states[current].hp, 0);
        assert_eq!(game.game_states[current].real_point, 0);
    }

    // === 正常伤害不触发游戏结束 ===

    #[test]
    fn test_no_game_over_on_survivable_damage() {
        let mut game = test_game(10);
        let current = game.current_player;
        game.game_states[current].hp = 6;
        game.do_effect_stacks.push_front(DoEffect::Action {
            source: Targeting::None,
            targeting: Targeting::TargetPlayerSelf,
            action: Action::Damage(3),
        });
        game.process_effect();
        assert!(game.game_over.is_none());
        assert_eq!(game.game_states[current].hp, 3);
    }

    // === effect队列中断测试 ===

    #[test]
    fn test_effect_queue_stops_after_game_over() {
        let mut game = test_game(10);
        let current = game.current_player;
        game.game_states[current].hp = 1;
        game.game_states[current].real_point = 0;
        // 先推入AddRealPoint，再推入致命Damage
        // process_effect 从前端取，所以先push Damage再push AddRealPoint
        // 但我们要测试：Damage导致game_over后，后续effect不再执行
        game.do_effect_stacks.push_back(DoEffect::Action {
            source: Targeting::None,
            targeting: Targeting::TargetPlayerSelf,
            action: Action::Damage(5),
        });
        game.do_effect_stacks.push_back(DoEffect::Action {
            source: Targeting::None,
            targeting: Targeting::TargetPlayerSelf,
            action: Action::AddRealPoint(100),
        });
        game.process_effect();
        assert!(game.game_over.is_some());
        // AddRealPoint 不应该被执行
        assert_eq!(game.game_states[current].real_point, 0);
    }
}
