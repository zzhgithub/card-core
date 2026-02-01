// 实体ID
pub type EntryId = usize;
pub type PlayerId = usize;
// 卡片定义id
pub type CardInfoId = String;
#[derive(Clone, Debug)]
pub struct IdGenerator(usize);

impl IdGenerator {
    pub fn new() -> Self {
        IdGenerator(0)
    }

    pub fn next(&mut self) -> usize {
        self.0 += 1;
        self.0
    }
}
