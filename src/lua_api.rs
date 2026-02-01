use crate::card::{CardInfo, CardInfoBuilder};
use mlua::prelude::{LuaError, LuaUserDataMethods};
use mlua::{AnyUserData, Function, Lua};
use std::collections::HashMap;

/// 脚本上下文

pub struct LuaApi {
    pub cards: HashMap<String, CardInfo>,
}

impl LuaApi {
    pub fn new() -> Self {
        Self {
            cards: HashMap::new(),
        }
    }

    // 初始化Lua脚本环境
    pub fn install(&mut self, lua: &Lua) -> Result<(), LuaError> {
        let mut api_ptr = std::ptr::NonNull::from(self);
        let define_card = lua.create_function_mut(move |lua, (id, func): (String, Function)| {
            let api = unsafe { api_ptr.as_mut() };

            let mut card = CardInfoBuilder::new(id.clone());

            let card_ud = lua.create_userdata(card)?;
            func.call::<()>(card_ud.clone())?;

            let card = card_ud.take::<CardInfoBuilder>()?;
            api.cards.insert(id, card.build());

            Ok(())
        })?;

        lua.globals().set("define_card", define_card)?;
        Ok(())
    }
}
