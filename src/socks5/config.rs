use super::AuthMethod;

pub struct Config {
    pub auth_method: Vec<AuthMethod>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            auth_method: vec![AuthMethod::NoAuth],
        }
    }
}
