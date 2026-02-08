use crate::choice_req::ChoiceReq;
use crate::choice_res::ChoiceRes;
use crate::common::EntryId;
use crate::effect::DoEffect::Action;
use crate::game::{Game, GamePhase};
use crate::player::Player;
use crate::player_actions::PlayerAction::Pass;
use crate::player_actions::{PlayerAction, ReadPlayerActions};
use crate::targeting::Targeting;
use crate::targeting::Targeting::TargetZone;
use log::{error as log_error, error, info, warn};
use std::cmp::PartialEq;
use std::io;

impl ReadPlayerActions for Game {
    fn read_action_main(&mut self) {
        while self.current_phase() == GamePhase::Main || self.current_phase() == GamePhase::Main2 {
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let tokens: Vec<_> = input.trim().split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }
            match tokens[0] {
                "help" => {
                    self.help_main();
                }
                "hp" => {
                    info!("hp: {:?}", self.current_hp());
                }
                "real" => {
                    info!("real: {:?}", self.current_real_point());
                }
                "hand" => {
                    info!("Hand {:?}", self.current_hand());
                }
                "cost" => {
                    info!("Cost {:?}", self.current_cost());
                }
                "zone" => {
                    info!("Zone {:?}", self.current_zone());
                }
                "desk" => {
                    info!("Desk left {:?} Cards", self.current_desk_len());
                }
                "grave" => {
                    info!("Grave {:?}", self.current_grave());
                }
                "look" => {
                    if tokens.len() == 2 {
                        let entry_id_str = tokens[1];
                        if let Ok(entry_id) = entry_id_str.parse() {
                            let card = self.get(entry_id);
                            info!("卡片详情 {:?}", card);
                        }
                    }
                }
                "set" => {
                    if tokens.len() == 3 {
                        // 创建指令
                        if let Ok(entry_id) = tokens[1].parse() {
                            if let Ok(zone_id) = tokens[2].parse() {
                                let action = PlayerAction::SetCard {
                                    card_id: entry_id,
                                    zone_id,
                                };
                                // 抛出Action
                                self.deal_player_action(action);
                            }
                        }
                    } else {
                        error!("Wrong number of arguments");
                    }
                }
                "pass" => {
                    break;
                }
                _ => {}
            }
        }
    }

    fn read_action_fight(&mut self) {
        while self.current_phase() == GamePhase::Fight {
            // 提示自己场上可以攻击的卡
            info!("look card Id 查看详情");
            info!("可以进行攻击的区域为[{:?}]", self.get_attack_zones());
            info!("对手场上可以被进攻的区域[{:?}]", self.get_attacked_zones());
            info!(
                "攻击，选取可以攻击的区域进攻某个其他区域\n\
            attack [zoneId] [zoneId] 自己进攻对手的区域\n\
            attack [zoneId] 直接攻击对手"
            );
            // 读取数据
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let tokens: Vec<_> = input.trim().split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }
            // 这里只生成两种的处理
            match tokens[0] {
                "look" => {
                    if tokens.len() == 2 {
                        let entry_id_str = tokens[1];
                        if let Ok(entry_id) = entry_id_str.parse() {
                            let card = self.get(entry_id);
                            info!("卡片详情 {:?}", card);
                        }
                    }
                }
                "attack" => {
                    // 这里处理攻击的对象问题
                    if tokens.len() == 3 {
                        // 取第二个和三个
                        let my_zone = tokens[1];
                        let opponent_zone = tokens[2];
                        if let Ok(my_zone_id) = my_zone.parse() {
                            if let Ok(opponent_zone_id) = opponent_zone.parse() {
                                self.deal_player_action(PlayerAction::AttackCard {
                                    source: TargetZone(my_zone_id),
                                    target: TargetZone(opponent_zone_id),
                                });
                            }
                        }
                    }
                    if tokens.len() == 2 {
                        // 值取源
                        let my_zone = tokens[1];
                        if let Ok(my_zone_id) = my_zone.parse() {
                            self.deal_player_action(PlayerAction::AttackCard {
                                source: TargetZone(my_zone_id),
                                target: Targeting::TargetPlayerOpponent,
                            });
                        }
                    }
                }
                "pass" => {
                    break;
                }
                _ => {}
            }
        }
    }

    // 选择是否使用伤害
    fn read_fight_damage(&mut self) -> ChoiceRes {
        loop {
            info!(
                "直接攻击玩家。是否消耗RealPoint对对手造成伤害,当前RealPoint[{:?}]",
                self.current_real_point()
            );
            info!("造成伤害[num]。放弃伤害，获得RealPoint: pass|0");
            // 读取数据
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let tokens: Vec<_> = input.trim().split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }
            if tokens.len() == 1 {
                if tokens[0] == "pass" {
                    // 跳过获取
                    break;
                }
                if let Ok(num) = tokens[0].parse() {
                    if num == 0 {
                        break;
                    }
                    // 这里进行处理
                    if num > self.current_real_point() {
                        error!("不能申请大于当前拥有的RealPoint");
                        continue;
                    }
                    return ChoiceRes::FightDamageByRealPoint(num);
                } else {
                    warn!("类型解析错误")
                }
            }
        }
        ChoiceRes::None
    }

    fn read_choice(&mut self, choice: ChoiceReq) -> ChoiceRes {
        match choice {
            ChoiceReq::Cost(card) => {
                // 检查费用是否足够
                if !self.check_cost(card) {
                    error!("无法支付费用,怎么返回原处");
                    self.set_rollback(card);
                    return ChoiceRes::None;
                }
                loop {
                    // 显示可以使用的数据
                    info!("Cost {:?}", self.current_cost());
                    info!("Hand {:?}", self.current_hand());
                    info!("Real Point:{:?}", self.current_real_point());
                    info!("Zone {:?}", self.current_zone());
                    info!(
                        "登场[{:?}]支付的费用为 {:?}",
                        self.get(card).card_info.clone().name,
                        self.get(card).card_info.clone().cost
                    );
                    info!(
                        "请选择 你要的支付费用的卡\
                        \n。[id1,id2,.. realPoint] 任意手卡id（逗号隔开）和realPoint的组合\
                        \n取消操作 cancel"
                    );
                    //FIXME ： 这里的阅读循环怎么优化重构？
                    let mut input = String::new();
                    io::stdin().read_line(&mut input).unwrap();
                    let tokens: Vec<_> = input.trim().split_whitespace().collect();
                    if tokens.is_empty() {
                        continue;
                    }
                    if tokens.len() == 1 && tokens[0] == "cancel" {
                        info!("取消操作");
                        self.set_rollback(card);
                        return ChoiceRes::None;
                    }
                    if tokens.len() == 2 {
                        let hands_str = tokens[0];
                        let hands: Vec<EntryId> = hands_str
                            .trim()
                            .split(",")
                            .map(|x| {
                                let Ok(r) = x.parse() else { todo!() };
                                r
                            })
                            .collect();
                        let point = tokens[1];
                        return if let Ok(point) = point.parse() {
                            ChoiceRes::Cost {
                                hands,
                                real_point: point,
                            }
                        } else {
                            ChoiceRes::Cost {
                                hands,
                                real_point: 0,
                            }
                        };
                    }
                }
            }
        }
    }

    fn help_main(&self) {
        info!(
            "Player {:?} 请操作:\n\
            help    帮助\n\
            hp      血量\n\
            real    真实点数\n\
            hand    查看手牌\n\
            zone    查看场地\n\
            cost    查看费用区\n\
            grave   查看墓地区\n \
            desk    查看卡组查看卡组剩余\n\
            set [entryId] [zoneId]\n\
            ",
            self.current_player()
        );
    }
}
