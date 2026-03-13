pub fn charge(amount: u64) -> bool {
    amount > 0
}

pub struct Invoice {
    pub amount: u64,
    pub paid: bool,
}
