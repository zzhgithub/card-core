use crate::common::EntryId;

#[derive(Debug, Clone)]
pub enum WindowEvent {
    // 支付费用的事件
    Cost { card: EntryId },
    // 登场事件
    Set { card: EntryId },
}
