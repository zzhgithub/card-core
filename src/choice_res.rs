use crate::common::EntryId;

#[derive(Debug, Clone)]
pub enum ChoiceRes {
    None,
    Cost {
        hands: Vec<EntryId>,
        real_point: usize,
    },
    FightDamageByRealPoint(usize),
}
