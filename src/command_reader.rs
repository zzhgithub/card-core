use crate::choice_req::ChoiceReq;
use crate::choice_res::ChoiceRes;
use crate::common::EntryId;
use crate::game::{Game, GamePhase};
use crate::player::Player;
use crate::player_actions::PlayerAction::Targeting;
use crate::player_actions::{PlayerAction, ReadPlayerActions};
use log::{error as log_error, error, info};
use std::cmp::PartialEq;
use std::io;

impl ReadPlayerActions for Game {
    fn read_action(&mut self, game_phase: GamePhase) {
        while self.current_phase() == game_phase {
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let tokens: Vec<_> = input.trim().split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }
            match tokens[0] {
                "help" => {
                    self.help();
                }
                "hp" => {
                    info!("hp: {:?}", self.current_hp());
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
                            info!("Looking for {:?}", card);
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
                    self.deal_player_action(PlayerAction::Pass);
                    break;
                }
                _ => {}
            }
        }
    }

    fn read_choice(&mut self, choice: ChoiceReq) -> ChoiceRes {
        match choice {
            ChoiceReq::Cost(card) => {
                // 检查费用是否足够
                if !self.check_cost(card) {
                    error!("无法支付费用，返回手卡");
                    self.to_hand(card);
                    return ChoiceRes::None;
                }
                loop {
                    // 显示可以使用的数据
                    info!("Cost {:?}", self.current_cost());
                    info!("Hand {:?}", self.current_hand());
                    info!("Real Point:{:?}", self.current_real_point());
                    info!("Zone {:?}", self.current_zone());
                    info!("请选择 你要的支付费用的卡。");
                    //FIXME ： 这里的阅读循环怎么优化重构？
                    let mut input = String::new();
                    io::stdin().read_line(&mut input).unwrap();
                    let tokens: Vec<_> = input.trim().split_whitespace().collect();
                    if tokens.is_empty() {
                        continue;
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

    fn help(&self) {
        info!(
            "Player {:?} 请操作:\n\
            help    帮助\n\
            hp      血量\n\
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
