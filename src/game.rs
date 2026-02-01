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
use log::{debug, info, trace, warn};
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

    // 进入手牌 不产生任何事件
    pub fn to_hand(&mut self, card: EntryId) {
        self.game_states[self.current_player].hand.push(card);
    }

    // 进入下一个阶段
    fn next_phase(&mut self) {
        self.current_phase = GamePhase::next(&self.current_phase)
    }

    pub fn cost(&mut self, hands: Vec<EntryId>, real_point: usize) {
        // 批量移动
        self.game_states[self.current_player]
            .hand
            .retain(|x| !hands.contains(x));
        self.game_states[self.current_player].cost.extend(hands);
        // 减少值
        self.game_states[self.current_player].real_point -= real_point;
    }

    pub fn deal_player_action(&mut self, player_acton: PlayerAction) {
        match player_acton {
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
            PlayerAction::Targeting { .. } => {}
            PlayerAction::AttackCard { .. } => {}
            PlayerAction::Pass => {
                self.next_phase();
            }
        }
    }

    pub fn emit_event(&mut self, window_event: WindowEvent) {
        match window_event {
            WindowEvent::Cost { card } => {
                todo!()
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

    pub fn current_phase(&self) -> GamePhase {
        self.current_phase
    }

    // 对手id
    fn next_player_id(&self) -> PlayerId {
        (self.current_player + 1) % self.players.len()
    }

    // 开始游戏
    pub fn run(&mut self) {
        loop {
            let player = &self.players[self.current_player];
            match self.current_phase {
                GamePhase::Start => {
                    info!("player[{:?}] 回合开始阶段", self.current_player);
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
                    self.help();
                    self.read_action(GamePhase::Main);
                    self.next_phase();
                }
                GamePhase::Fight => {
                    info!("player[{:?}] 战斗阶段", self.current_player);
                    self.next_phase();
                }
                GamePhase::Main2 => {
                    info!("player[{:?}] 主要阶段2", self.current_player);
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
}
