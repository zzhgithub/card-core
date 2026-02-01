use crate::common::EntryId;
use crate::targeting::Targeting;
use mlua::{UserData, UserDataMethods};

/// TODO 这里讨论效果的详情
///
/// 1. id
/// 2. 发动的条件
/// 3. 发动的窗口 window 和游戏的阶段有关
/// 4. 成功后的动作 Option_Action {target目标（选取范围）}
/// 5. 频率限制
///

/// 效果类
#[derive(Debug, Clone, Default)]
pub struct Effect {
    id: String,
    windows_tag: WindowsTag,
    condition: Condition,
    do_effect: DoEffect,
    // TODO 这里要处理一下选择的问题
}

#[derive(Debug, Clone, Default)]
pub struct EffectBuilder {
    id: String,
    windows_tag: WindowsTag,
    condition: Condition,
    do_effect: DoEffect,
}

impl EffectBuilder {
    pub fn new(id: String) -> EffectBuilder {
        Self {
            id,
            ..Default::default()
        }
    }

    pub fn build(&self) -> Effect {
        Effect {
            id: self.id.clone(),
            windows_tag: self.windows_tag.clone(),
            condition: self.condition.clone(),
            do_effect: self.do_effect.clone(),
        }
    }
}

impl UserData for EffectBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // 添加方法
        // 这要添加方法

        // 添加操作窗口标签
        methods.add_method_mut("window", |lua, this, tag: String| {
            match tag.as_str() {
                "self_start" => this.windows_tag = WindowsTag::OnSelfStart,
                "opponent_start" => this.windows_tag = WindowsTag::OnOpponentStart,
                "start" => this.windows_tag = WindowsTag::OnStart,
                "cost" => this.windows_tag = WindowsTag::OnCost,
                "set" => this.windows_tag = WindowsTag::OnSet,
                "main" => this.windows_tag = WindowsTag::DuringMain,
                "attack" => this.windows_tag = WindowsTag::OnAttack,
                _ => {}
            }
            Ok(())
        });

        methods.add_method_mut("draw", |_, this, num: usize| {
            let effect = DoEffect::Action {
                source: Default::default(),
                targeting: Targeting::TargetPlayerSelf,
                action: Action::Draw(num),
            };
            this.do_effect = effect;
            Ok(())
        });
    }
}

/// 窗口标签
#[derive(Debug, Clone, Default)]
pub enum WindowsTag {
    #[default]
    None,
    // 自己回合开始
    OnSelfStart,
    // 对手回合开始
    OnOpponentStart,
    // 回合开始阶段
    OnStart,
    // 暴露时
    OnCost,
    // 登场时
    OnSet,
    // 主要阶段
    DuringMain,
    // 攻击时
    OnAttack,
}

#[derive(Debug, Clone, Default)]
pub enum DoEffect {
    // 操作
    #[default]
    None,
    Action {
        source: Targeting,
        targeting: Targeting,
        action: Action,
    },
    // n选1
    AndAction(Vec<DoEffect>),
    // 任意操作
    OrAction(Vec<DoEffect>),
}

/// 条件
#[derive(Debug, Clone, Default)]
pub enum Condition {
    #[default]
    None,
}

/// 操作效果
#[derive(Debug, Clone, Default)]
pub enum Action {
    // 没有任何操作
    #[default]
    None,
    // 抽卡
    Draw(usize),
    // 从手牌设置卡片
    Set {
        card_id: EntryId,
        zone_id: EntryId,
    },
}
