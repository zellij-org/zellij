pub use super::generated_api::api::pane_frame_style::PaneFrameStyle as ProtobufPaneFrameStyle;
use crate::input::options::PaneFrameStyle;

use std::convert::TryFrom;

impl TryFrom<ProtobufPaneFrameStyle> for PaneFrameStyle {
    type Error = &'static str;
    fn try_from(protobuf_pane_frame_style: ProtobufPaneFrameStyle) -> Result<Self, &'static str> {
        match protobuf_pane_frame_style {
            ProtobufPaneFrameStyle::Full => Ok(PaneFrameStyle::Full),
            ProtobufPaneFrameStyle::Titles => Ok(PaneFrameStyle::Titles),
            ProtobufPaneFrameStyle::None => Ok(PaneFrameStyle::None),
        }
    }
}

impl TryFrom<PaneFrameStyle> for ProtobufPaneFrameStyle {
    type Error = &'static str;
    fn try_from(pane_frame_style: PaneFrameStyle) -> Result<Self, &'static str> {
        Ok(match pane_frame_style {
            PaneFrameStyle::Full => ProtobufPaneFrameStyle::Full,
            PaneFrameStyle::Titles => ProtobufPaneFrameStyle::Titles,
            PaneFrameStyle::None => ProtobufPaneFrameStyle::None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_frame_style_roundtrips_through_protobuf() {
        for style in [
            PaneFrameStyle::Full,
            PaneFrameStyle::Titles,
            PaneFrameStyle::None,
        ] {
            let protobuf: ProtobufPaneFrameStyle = style.try_into().unwrap();
            let roundtripped: PaneFrameStyle = protobuf.try_into().unwrap();
            assert_eq!(style, roundtripped);
        }
    }

    #[test]
    fn unknown_pane_frame_style_value_is_rejected() {
        assert!(ProtobufPaneFrameStyle::from_i32(99).is_none());
    }
}
