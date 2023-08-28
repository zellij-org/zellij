// NOTE: This file is generated automatically, do *NOT* edit it by hand!
// Refer to [the PR introducing this change][1] to learn more about the reasons.
//
// [1]: https://github.com/zellij-org/zellij/pull/2711#issuecomment-1695015818
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum PermissionType {
    ReadApplicationState = 0,
    ChangeApplicationState = 1,
    OpenFiles = 2,
    RunCommands = 3,
    OpenTerminalsOrPlugins = 4,
    WriteToStdin = 5,
}
impl PermissionType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            PermissionType::ReadApplicationState => "ReadApplicationState",
            PermissionType::ChangeApplicationState => "ChangeApplicationState",
            PermissionType::OpenFiles => "OpenFiles",
            PermissionType::RunCommands => "RunCommands",
            PermissionType::OpenTerminalsOrPlugins => "OpenTerminalsOrPlugins",
            PermissionType::WriteToStdin => "WriteToStdin",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "ReadApplicationState" => Some(Self::ReadApplicationState),
            "ChangeApplicationState" => Some(Self::ChangeApplicationState),
            "OpenFiles" => Some(Self::OpenFiles),
            "RunCommands" => Some(Self::RunCommands),
            "OpenTerminalsOrPlugins" => Some(Self::OpenTerminalsOrPlugins),
            "WriteToStdin" => Some(Self::WriteToStdin),
            _ => None,
        }
    }
}
