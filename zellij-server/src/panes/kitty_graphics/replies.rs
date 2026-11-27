use super::grid_state::KittyReplyData;
use super::parser::KittyError;

fn keys(image_id: Option<u32>, image_number: Option<u32>, placement_id: Option<u32>) -> String {
    let mut components = Vec::new();
    if let Some(image_id) = image_id {
        components.push(format!("i={}", image_id));
    }
    if let Some(image_number) = image_number {
        components.push(format!("I={}", image_number));
    }
    if let Some(placement_id) = placement_id {
        components.push(format!("p={}", placement_id));
    }
    components.join(",")
}

fn wrap(keys: String, status: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"\x1b_G");
    out.extend_from_slice(keys.as_bytes());
    out.push(b';');
    out.extend_from_slice(status);
    out.extend_from_slice(b"\x1b\\");
    out
}

pub fn format_kitty_reply(reply: &KittyReplyData, was_query: bool) -> Option<Vec<u8>> {
    if reply.quiet >= 1 {
        return None;
    }
    if !was_query
        && !(reply.image_number.is_some() || reply.image_id.map(|id| id != 0).unwrap_or(false))
    {
        return None;
    }
    Some(wrap(
        keys(reply.image_id, reply.image_number, reply.placement_id),
        b"OK",
    ))
}

pub fn format_kitty_error(error: &KittyError) -> Option<Vec<u8>> {
    if error.quiet >= 2 {
        return None;
    }
    Some(wrap(
        keys(error.image_id, error.image_number, error.placement_id),
        format!("{}:{}", error.code.as_str(), error.message).as_bytes(),
    ))
}
