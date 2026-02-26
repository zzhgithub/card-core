use crate::card_loader::load_cards;
use crate::lua_api::LuaApi;
use mlua::Lua;
use std::collections::HashSet;
use std::env;
mod ai;
mod card;
mod card_loader;
mod choice_req;
mod choice_res;
mod command_reader;
mod common;
mod desk_loader;
mod effect;
mod game;
mod game_diff;
mod lua_api;
mod player;
mod player_actions;
mod targeting;
mod window_event;

use crate::desk_loader::load_desks;
use crate::effect::EffectBuilder;
use crate::game::Game;
use crate::player::Player;
use log::{debug, error, info, log_enabled, Level};

fn main() {
    env_logger::builder()
        .target(env_logger::Target::Stdout)
        .filter_level(log::LevelFilter::Info)
        .try_init()
        .unwrap();

    let args: Vec<String> = env::args().collect();
    let mut ai_players: HashSet<usize> = HashSet::new();

    for i in 0..args.len() {
        if args[i] == "--ai" && i + 1 < args.len() {
            let ai_arg = &args[i + 1];
            for c in ai_arg.chars() {
                if let Some(player_id) = c.to_digit(10) {
                    ai_players.insert(player_id as usize);
                }
            }
        }
    }

    if !ai_players.is_empty() {
        info!("AI players: {:?}", ai_players);
    }

    info!("Start!");
    let mut api = LuaApi::new();
    let mut lua = Lua::new();
    info!("Loading cards...");
    load_cards(&lua, &mut api);
    info!("cards loaded!");
    info!("All {:?}", api.cards);

    info!("Loading desk");
    let desks = load_desks();
    info!("desk loaded!");

    if let Some(desk_test) = desks.get("test1") {
        let player_0 = Player {
            id: 0,
            player_desk: desk_test.clone(),
        };
        let player_1 = Player {
            id: 1,
            player_desk: desk_test.clone(),
        };

        let mut game = Game::new(vec![player_0, player_1], &api, ai_players);
        debug!("Game {:?}", game);
        game.run();
    }
}
