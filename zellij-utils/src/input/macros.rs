use std::collections::HashMap;
use super::actions::Action;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Macros(pub HashMap<String, Vec<Action>>);

impl Macros {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn get_actions(&self, macro_name: &str) -> Option<&Vec<Action>> {
        self.0.get(macro_name)
    }

    pub fn insert(&mut self, name: String, actions: Vec<Action>) {
        self.0.insert(name, actions);
    }

    pub fn remove(&mut self, name: &str) -> Option<Vec<Action>> {
        self.0.remove(name)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn merge(&mut self, other: Macros) {
        for (name, actions) in other.0.into_iter() {
            self.0.insert(name, actions);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::actions::Action;

    #[test]
    fn test_new_macros_is_empty() {
        let macros = Macros::new();
        assert!(macros.is_empty());
        assert_eq!(macros.len(), 0);
    }

    #[test]
    fn test_default_macros_is_empty() {
        let macros = Macros::default();
        assert!(macros.is_empty());
        assert_eq!(macros.len(), 0);
    }

    #[test]
    fn test_insert_and_get() {
        let mut macros = Macros::new();
        let actions = vec![Action::Quit];
        macros.insert("test_macro".to_string(), actions.clone());

        assert_eq!(macros.len(), 1);
        assert_eq!(macros.get_actions("test_macro"), Some(&actions));
    }

    #[test]
    fn test_insert_multiple_actions() {
        let mut macros = Macros::new();
        let actions = vec![Action::Quit, Action::Detach];
        macros.insert("multi_action_macro".to_string(), actions.clone());

        assert_eq!(macros.len(), 1);
        assert_eq!(macros.get_actions("multi_action_macro"), Some(&actions));
    }

    #[test]
    fn test_get_nonexistent_macro() {
        let macros = Macros::new();
        assert_eq!(macros.get_actions("nonexistent"), None);
    }

    #[test]
    fn test_remove() {
        let mut macros = Macros::new();
        macros.insert("test_macro".to_string(), vec![Action::Quit]);

        let removed = macros.remove("test_macro");
        assert_eq!(removed, Some(vec![Action::Quit]));
        assert!(macros.is_empty());
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut macros = Macros::new();
        let removed = macros.remove("nonexistent");
        assert_eq!(removed, None);
    }

    #[test]
    fn test_insert_overwrites_existing() {
        let mut macros = Macros::new();
        macros.insert("test_macro".to_string(), vec![Action::Quit]);
        macros.insert("test_macro".to_string(), vec![Action::Detach]);

        assert_eq!(macros.len(), 1);
        assert_eq!(macros.get_actions("test_macro"), Some(&vec![Action::Detach]));
    }

    #[test]
    fn test_merge_adds_new_macros() {
        let mut macros1 = Macros::new();
        macros1.insert("macro1".to_string(), vec![Action::Quit]);

        let mut macros2 = Macros::new();
        macros2.insert("macro2".to_string(), vec![Action::Detach]);

        macros1.merge(macros2);

        assert_eq!(macros1.len(), 2);
        assert_eq!(macros1.get_actions("macro1"), Some(&vec![Action::Quit]));
        assert_eq!(macros1.get_actions("macro2"), Some(&vec![Action::Detach]));
    }

    #[test]
    fn test_merge_overwrites_shared_macros() {
        let mut macros1 = Macros::new();
        macros1.insert("macro1".to_string(), vec![Action::Quit]);
        macros1.insert("shared".to_string(), vec![Action::Quit]);

        let mut macros2 = Macros::new();
        macros2.insert("macro2".to_string(), vec![Action::Detach]);
        macros2.insert("shared".to_string(), vec![Action::Detach]);

        macros1.merge(macros2);

        assert_eq!(macros1.len(), 3);
        assert_eq!(macros1.get_actions("macro1"), Some(&vec![Action::Quit]));
        assert_eq!(macros1.get_actions("macro2"), Some(&vec![Action::Detach]));
        assert_eq!(macros1.get_actions("shared"), Some(&vec![Action::Detach]));
    }

    #[test]
    fn test_merge_empty_into_populated() {
        let mut macros1 = Macros::new();
        macros1.insert("macro1".to_string(), vec![Action::Quit]);

        let macros2 = Macros::new();

        macros1.merge(macros2);

        assert_eq!(macros1.len(), 1);
        assert_eq!(macros1.get_actions("macro1"), Some(&vec![Action::Quit]));
    }

    #[test]
    fn test_merge_into_empty() {
        let mut macros1 = Macros::new();

        let mut macros2 = Macros::new();
        macros2.insert("macro2".to_string(), vec![Action::Detach]);

        macros1.merge(macros2);

        assert_eq!(macros1.len(), 1);
        assert_eq!(macros1.get_actions("macro2"), Some(&vec![Action::Detach]));
    }

    #[test]
    fn test_clone() {
        let mut macros1 = Macros::new();
        macros1.insert("test_macro".to_string(), vec![Action::Quit]);

        let macros2 = macros1.clone();

        assert_eq!(macros1, macros2);
        assert_eq!(macros2.get_actions("test_macro"), Some(&vec![Action::Quit]));
    }
}
