use super::*;

impl Press {
    pub fn with_text(mut self, text: &str) -> Self {
        self.text = Some(text.to_owned());
        self
    }
}

pub fn translate(press: &Press) -> Option<KeyWithModifier> {
    match typed(press) {
        Some(Typed::Key(key)) => Some(key),
        _ => None,
    }
}
