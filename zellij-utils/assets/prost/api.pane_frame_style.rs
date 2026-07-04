#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum PaneFrameStyle {
    Full = 0,
    Titles = 1,
    None = 2,
}
impl PaneFrameStyle {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            PaneFrameStyle::Full => "Full",
            PaneFrameStyle::Titles => "Titles",
            PaneFrameStyle::None => "None",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Full" => Some(Self::Full),
            "Titles" => Some(Self::Titles),
            "None" => Some(Self::None),
            _ => None,
        }
    }
}
