mod auth;
use sample::billing;

fn main() {
    auth::login("user", "pass");
    billing::charge(100);
}
