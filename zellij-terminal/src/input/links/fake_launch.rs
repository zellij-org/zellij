thread_local! {
    static OPENED: std::cell::RefCell<Vec<(String, String)>> =
        std::cell::RefCell::new(Vec::new());
}

pub(crate) fn launch(opener: &str, uri: &str) -> Result<(), String> {
    OPENED.with(|opened| {
        opened
            .borrow_mut()
            .push((opener.to_owned(), uri.to_owned()))
    });
    Ok(())
}

pub fn opened() -> Vec<(String, String)> {
    OPENED.with(|opened| opened.borrow().clone())
}

pub fn forget_opened() {
    OPENED.with(|opened| opened.borrow_mut().clear());
}
