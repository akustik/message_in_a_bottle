use std::env;

pub fn env_or_fail(name: &str) -> String {
    env::var(name).expect(&format!("${}", name))
}