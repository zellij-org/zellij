use std::cell::RefCell;

use zellij_tile::prelude::actions::Action;

thread_local! {
    static HOVERED: RefCell<Option<Vec<Action>>> = RefCell::new(None);
}

pub fn with_hovered<T>(hovered: Option<Vec<Action>>, render: impl FnOnce() -> T) -> T {
    HOVERED.with(|h| *h.borrow_mut() = hovered);
    let result = render();
    HOVERED.with(|h| *h.borrow_mut() = None);
    result
}

pub fn is_hovered(actions: &[Action]) -> bool {
    HOVERED.with(|h| h.borrow().as_deref() == Some(actions))
}
