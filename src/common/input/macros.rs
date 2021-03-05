//! Use a list of commands and execute them in a
//! defined predictable order.

use super::actions::Action;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Macro {
    name: Option<String>,
    sequence: Vec<Action>,
}
