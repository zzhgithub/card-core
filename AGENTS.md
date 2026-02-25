# AGENTS.md

## 项目概述

这是一个卡牌游戏核心库，使用 Rust 编写，使用 mlua 集成 Lua 脚本引擎。游戏逻辑通过 Lua 脚本定义卡片和效果。

## 构建与运行

```bash
# 构建项目
cargo build

# 运行项目
cargo run

# 运行并显示详细日志
cargo run -- -v

# 检查代码（lint + format）
cargo clippy
cargo fmt --check

# 格式化代码
cargo fmt
```

## 测试

当前项目暂无单元测试。

```bash
# 运行所有测试（当前无测试）
cargo test
```

## 代码风格规范

### 1. 文件组织

- 模块声明按字母顺序排列（`mod aaa; mod bbb;`）
- 每个文件约 100-300 行
- 功能相关的代码放在同一模块

### 2. Import 导入顺序

```rust
// 标准库
use std::collections::VecDeque;
use std::sync::Arc;

// 外部 crate
use log::{debug, error, info, warn};
use mlua::{Function, Lua, UserData};

// crate 内部模块
use crate::card::Card;
use crate::game::Game;
```

### 3. 命名规范

| 类型 | 命名规则 | 示例 |
|------|----------|------|
| 结构体/枚举 | PascalCase | `GamePhase`, `Zone`, `CardInfoBuilder` |
| 函数/方法 | snake_case | `next_phase()`, `get_card()` |
| 变量 | snake_case | `card_info`, `entry_id` |
| 枚举成员 | PascalCase | `GamePhase::Start`, `Zone::FrontEnd` |
| 常量 | SCREAMING_SNAKE_CASE | `MAX_HAND_SIZE` |
| 类型别名 | PascalCase | `EntryId = usize`, `PlayerId = usize` |

### 4. 结构体字段顺序

```rust
// 公有字段在前，私有字段在后
pub struct Card {
    pub entry_id: EntryId,
    pub card_info: CardInfo,
    // 私有字段
    attack_counter: usize,
    attack_max: usize,
}
```

### 5. Builder 模式

复杂对象使用 Builder 模式：

```rust
// 私有字段，外部无法直接创建
pub struct CardInfoBuilder {
    id: CardInfoId,
    name: String,
    cost: usize,
    // ...
}

impl CardInfoBuilder {
    pub fn new(id: String) -> Self { ... }
    pub fn build(self) -> CardInfo { ... }
}
```

### 6. 错误处理

- 使用 `Result<T, E>` 进行显式错误处理
- 避免使用 `unwrap()`，除非是确定不会失败的场景
- Lua API 使用 `mlua::prelude::LuaError`

```rust
pub fn install(&mut self, lua: &Lua) -> Result<(), LuaError> {
    // 错误传播使用 ? 运算符
    lua.globals().set("define_card", define_card)?;
    Ok(())
}
```

### 7. 注释规范

- **不使用注释**，除非是公开 API 文档或解释复杂逻辑
- 模块级文档注释使用 `///`
- 避免行内注释

```rust
/// 游戏阶段
#[derive(Clone, Debug)]
pub enum GamePhase {
    Start,
    Draw,
    // ...
}
```

### 8. 宏和属性

```rust
#[derive(Clone, Debug)]
pub struct Game { ... }

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum GamePhase { ... }
```

### 9. Lua API 集成

Lua API 通过 `mlua` 的 `UserData` 实现：

```rust
impl UserData for CardInfoBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("name", |lua, this, name: String| {
            this.name = name;
            Ok(())
        });
    }
}
```

方法命名与 Lua 脚本中的方法名一致（如 `card:name()` 对应 Rust 中的 `add_method_mut("name", ...)`）。

### 10. 枚举使用

使用完整路径或模块前缀区分枚举成员：

```rust
match phase {
    GamePhase::Start => GamePhase::Draw,
    GamePhase::Draw => GamePhase::Reuse,
    // ...
}
```

### 11. 容器类型选择

| 场景 | 类型 |
|------|------|
| 键值对查找 | `HashMap<K, V>` |
| 有序迭代 | `Vec<T>` |
| 栈/队列 | `VecDeque<T>` |
| 唯一元素 | `HashSet<T>` |

### 12. 日志规范

使用 `log` crate 的宏，按需选择级别：

```rust
error!("错误信息");   // 错误
warn!("警告信息");    // 警告
info!("信息");       // 一般信息
debug!("调试信息");   // 调试
trace!("跟踪信息");   // 详细跟踪
```

## Lua 脚本规范

### 卡片定义

```lua
define_card("卡片ID", function(card)
    card:name("卡片名称")
    card:cost(费用)
    card:ack(攻击力)
    card:reg_effect("效果ID", function(effect)
        effect:window("窗口标签")
        -- 效果具体实现
    end)
end)
```

### 窗口标签可选值

- `"self_start"` - 自己回合开始
- `"opponent_start"` - 对手回合开始
- `"start"` - 回合开始阶段
- `"cost"` - 费用支付
- `"set"` - 登场时
- `"main"` - 主要阶段
- `"attack"` - 攻击时

## 项目结构

```
card-core/
├── src/
│   ├── main.rs           # 入口
│   ├── game.rs           # 游戏核心逻辑
│   ├── card.rs           # 卡片定义
│   ├── effect.rs         # 效果定义
│   ├── lua_api.rs        # Lua API 入口
│   ├── player.rs         # 玩家
│   └── ...
├── cards/                # Lua 卡片脚本
│   └── S000-A-001.lua
└── Cargo.toml
```

## 常用命令速查

```bash
cargo build          # 构建
cargo run            # 运行
cargo clippy         # 代码检查
cargo fmt            # 格式化
cargo check          # 仅检查类型
cargo doc --open     # 生成文档
```
