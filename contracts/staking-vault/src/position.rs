use soroban_sdk::contracttype;

pub const LOYALTY_DAYS: i128 = 180;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    pub day: u64,
    pub principal: i128,
    pub age_credit: i128,
    pub pending_deposit: i128,
    pub pending_exit: i128,
    pub collectible: i128,
}

impl Position {
    pub fn empty(day: u64) -> Self {
        Self {
            day, principal: 0, age_credit: 0,
            pending_deposit: 0, pending_exit: 0, collectible: 0,
        }
    }

    pub fn power(&self) -> i128 {
        self.principal * LOYALTY_DAYS + self.age_credit
    }

    fn age(&mut self, to: u64) {
        let elapsed = i128::from(to - self.day).min(LOYALTY_DAYS);
        self.age_credit = (self.age_credit + elapsed * self.principal)
            .min(LOYALTY_DAYS * self.principal);
        self.day = to;
    }

    pub fn advance(&mut self, to: u64) {
        if to == self.day {
            return;
        }
        self.age(self.day + 1);
        if self.pending_exit > 0 {
            let retained = self.principal - self.pending_exit;
            self.age_credit = self.age_credit * retained / self.principal;
            self.principal = retained;
            self.collectible += self.pending_exit;
            self.pending_exit = 0;
        }
        self.principal += self.pending_deposit;
        self.pending_deposit = 0;
        self.age(to);
    }
}
