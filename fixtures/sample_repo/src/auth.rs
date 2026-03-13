pub fn login(user: &str, pass: &str) -> bool {
    validate(user, pass)
}

fn validate(user: &str, _pass: &str) -> bool {
    !user.is_empty()
}

pub struct Session {
    pub user: String,
    pub token: String,
}
