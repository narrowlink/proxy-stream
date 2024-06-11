#[derive(Default)]
pub enum AuthMethod {
    #[default]
    NoAuth,
}

#[derive(Default)]
#[allow(dead_code)]
pub struct Config {
    pub auth_method: AuthMethod,
}
