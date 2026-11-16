use std::cell::RefCell;

use zellij_tile::prelude::*;

thread_local! {
    static COLORS: RefCell<Vec<(AttentionStatus, [u8; 3])>> = RefCell::new(vec![]);
}

pub fn of(status: AttentionStatus) -> Option<[u8; 3]> {
    COLORS.with(|colors| {
        colors
            .borrow()
            .iter()
            .find(|(s, _)| *s == status)
            .map(|(_, rgb)| *rgb)
    })
}

pub fn set(colors: &[(AttentionStatus, [u8; 3])]) {
    COLORS.with(|stored| *stored.borrow_mut() = colors.to_vec());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_colours_sent_with_the_attention_update_are_used() {
        set(&[(AttentionStatus::Failed, [0x8e, 0x4e, 0xc6])]);
        assert_eq!(of(AttentionStatus::Failed), Some([0x8e, 0x4e, 0xc6]));
        assert_eq!(of(AttentionStatus::Done), None);
        set(&[]);
        assert_eq!(of(AttentionStatus::Failed), None);
    }
}
