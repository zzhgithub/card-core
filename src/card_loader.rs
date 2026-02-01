use crate::lua_api::LuaApi;
use mlua::Lua;
use std::fs;

pub fn load_cards(lua: &Lua, api: &mut LuaApi) {
    api.install(lua).unwrap();

    for file in fs::read_dir("cards").unwrap() {
        let code = fs::read_to_string(file.unwrap().path()).unwrap();
        lua.load(&code).exec().unwrap();
    }
}
