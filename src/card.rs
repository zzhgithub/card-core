use crate::common::{CardInfoId, EntryId, IdGenerator};
use crate::effect::{Effect, EffectBuilder};
use crate::lua_api::LuaApi;
use crate::player::PlayerDesk;
use log::warn;
use mlua::{Function, Lua, UserData, UserDataMethods};

/// 卡片信息
#[derive(Debug, Clone)]
pub struct CardInfo {
    // id
    id: CardInfoId,
    // 卡片名称
    pub name: String,
    // 花费
    pub cost: usize,
    // 攻击力
    pub ack: usize,
    // 效果
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone)]
pub struct CardInfoBuilder {
    // id
    id: CardInfoId,
    // 卡片名称
    name: String,
    // 花费
    cost: usize,
    // 攻击力
    ack: usize,
    // 效果
    effects: Vec<Effect>,
}

impl CardInfoBuilder {
    pub fn new(id: String) -> Self {
        Self {
            id,
            name: "".to_string(),
            cost: 0,
            ack: 0,
            effects: Vec::new(),
        }
    }

    pub fn build(self) -> CardInfo {
        CardInfo {
            id: self.id,
            name: self.name,
            cost: self.cost,
            ack: self.ack,
            effects: self.effects,
        }
    }
}

impl UserData for CardInfoBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // 添加基础信息的方法
        methods.add_method_mut("name", |lua, this, name: String| {
            this.name = name;
            Ok(())
        });

        methods.add_method_mut("cost", |lua, this, cost: usize| {
            this.cost = cost;
            Ok(())
        });

        methods.add_method_mut("ack", |lua, this, ack: usize| {
            this.ack = ack;
            Ok(())
        });

        // 添加效果的方法
        methods.add_method_mut("reg_effect", |lua, this, (id, func): (String, Function)| {
            let builder = EffectBuilder::new(id);

            let effect_ud = lua.create_userdata(builder)?;
            func.call::<()>(effect_ud.clone())?;

            let effect_builder = effect_ud.take::<EffectBuilder>()?;
            this.effects.push(effect_builder.build());

            Ok(())
        });
    }
}

/// 卡片对象
#[derive(Debug, Clone)]
pub struct Card {
    pub entry_id: EntryId,
    pub card_info: CardInfo,
}

impl Card {
    // 初始化列表
    pub fn init(
        player_desk: PlayerDesk,
        lua_api: &LuaApi,
        id_generator: &mut IdGenerator,
    ) -> Vec<Card> {
        let mut res = Vec::new();
        for card_info_id in player_desk.0 {
            if let Some(card_info) = lua_api.cards.get(&card_info_id) {
                res.push(Card {
                    entry_id: id_generator.next(),
                    card_info: card_info.clone(),
                });
            } else {
                warn!("Card with id {} not found", card_info_id);
            }
        }
        res
    }
}
