use crate::common::EntryId;

#[derive(Debug, Clone)]
pub enum ChoiceReq {
    Cost(EntryId),
}
