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
use std::collections::VecDeque;
use std::sync::Arc;

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
}

impl Game {
    // 创建游戏
    pub fn new(players: Vec<Player>, lua_api: &LuaApi) -> Self {
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
        }
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

    // 检查 卡片当前费用是否足够
    pub fn check_cost(&self, card_id: EntryId) -> bool {
        // （手牌的个数 ， cost局域剩余） 最小值 + real_point <= cost
        let can_cost = self.game_states[self.current_player]
            .hand
            .len()
            .min(6 - self.current_cost().len())
            + self.current_real_point();
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
        // 批量移动
        self.game_states[self.current_player]
            .hand
            .retain(|x| !hands.contains(x));
        self.game_states[self.current_player].cost.extend(hands);
        // 减少值
        self.game_states[self.current_player].real_point -= real_point;
        // TODO 这里抛出一系列事件 给后续自己排连锁
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
                    self.read_choice(ChoiceReq::Cost(card_id))
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
                            self.game_states[self.current_player].draw(num);
                        }
                        Targeting::TargetPlayerOpponent => {
                            let i = self.next_player_id();
                            self.game_states[i].draw(num);
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
                                    // TODO 抛出获胜事件 终止游戏
                                    info!(
                                        "生命+RealPoint不足 {:?} 获胜",
                                        Targeting::TargetPlayerOpponent
                                    );
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
                                    // TODO 抛出获胜事件 终止游戏
                                    info!(
                                        "生命+RealPoint不足 {:?} 获胜",
                                        Targeting::TargetPlayerSelf
                                    );
                                }
                            } else {
                                self.game_states[next_id].hp -= num;
                                info!("伤害后的生命值hp[{:?}]", self.game_states[next_id].hp);
                            }
                        }
                    }
                    Action::AddRealPoint(num) => {
                        if let Targeting::TargetPlayerSelf = targeting {
                            self.game_states[self.current_player].real_point += num;
                            // todo 这里的要考虑溢出的情况
                            info!("当前玩家RealPoint 增加[{:?}]", num);
                        }
                        if let Targeting::TargetPlayerOpponent = targeting {
                            let next_id = self.next_player_id();
                            self.game_states[next_id].real_point += num;
                            // todo 这里的要考虑溢出的情况
                            info!("对方玩家RealPoint 增加[{:?}]", num);
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
    fn next_player_id(&self) -> PlayerId {
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
                    self.next_phase();
                }
                GamePhase::Reuse => {
                    info!("player[{:?}] 回收阶段", self.current_player);
                    // todo
                    self.next_phase();
                }
                GamePhase::Main => {
                    info!("player[{:?}] 主要阶段1", self.current_player);
                    self.help_main();
                    self.read_action_main();
                    self.next_phase();
                }
                GamePhase::Fight => {
                    info!("player[{:?}] 战斗阶段", self.current_player);
                    self.read_action_fight();
                    self.next_phase();
                }
                GamePhase::Main2 => {
                    info!("player[{:?}] 主要阶段2", self.current_player);
                    self.help_main();
                    self.read_action_main();
                    self.next_phase();
                }
                GamePhase::End => {
                    info!("player[{:?}] 回合结束阶段", self.current_player);
                    self.next_phase();
                    self.current_player = self.next_player_id();
                }
            }
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

    // 抽卡
    pub fn draw(&mut self, num: usize) {
        if num > self.desk.len() {
            warn!("卡组剩余卡不能抽完，需要抛出事件")
        }
        for _ in 0..num {
            if let Some(entry_id) = self.desk.pop() {
                self.hand.push(entry_id);
            }
        }
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
