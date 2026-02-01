use crate::card_loader::load_cards;
use crate::lua_api::LuaApi;
use mlua::Lua;
mod card;
mod card_loader;
mod command_reader;
mod common;
mod desk_loader;
mod effect;
mod game;
mod lua_api;
mod player;
mod player_actions;
mod targeting;
mod game_diff;
mod choice_req;
mod choice_res;

use crate::desk_loader::load_desks;
use crate::effect::EffectBuilder;
use crate::game::Game;
use crate::player::Player;
use log::{Level, debug, error, info, log_enabled};

fn main() {
    env_logger::builder()
        .target(env_logger::Target::Stdout)
        .try_init()
        .unwrap();
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

        let mut game = Game::new(vec![player_0, player_1], &api);
        debug!("Game {:?}", game);
        game.run();
    }
}
