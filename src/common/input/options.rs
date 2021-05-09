use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, PartialEq, Deserialize, Serialize)]
/// Options that can be set either through the config file,
/// or cli flags
pub struct Options {
    /// Allow plugins to use a more compatible font type
    pub simplified_ui: bool,
}

impl Options {
    pub fn from_yaml(from_yaml: Option<Options>) -> Options {
        if let Some(opts) = from_yaml {
            opts
        } else {
            Options::default()
        }
    }
}
