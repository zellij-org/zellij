pub use super::generated_api::api::plugin_permission::PermissionType as ProtobufPermissionType;
use crate::data::PermissionType;

use std::convert::TryFrom;

impl TryFrom<ProtobufPermissionType> for PermissionType {
    type Error = &'static str;
    fn try_from(protobuf_permission: ProtobufPermissionType) -> Result<Self, &'static str> {
        match protobuf_permission {
            ProtobufPermissionType::ReadApplicationState => {
                Ok(PermissionType::ReadApplicationState)
            },
            ProtobufPermissionType::ChangeApplicationState => {
                Ok(PermissionType::ChangeApplicationState)
            },
            ProtobufPermissionType::OpenFiles => Ok(PermissionType::OpenFiles),
            ProtobufPermissionType::RunCommands => Ok(PermissionType::RunCommands),
            ProtobufPermissionType::OpenTerminalsOrPlugins => {
                Ok(PermissionType::OpenTerminalsOrPlugins)
            },
            ProtobufPermissionType::WriteToStdin => Ok(PermissionType::WriteToStdin),
            ProtobufPermissionType::WebAccess => Ok(PermissionType::WebAccess),
            ProtobufPermissionType::ReadCliPipes => Ok(PermissionType::ReadCliPipes),
            ProtobufPermissionType::MessageAndLaunchOtherPlugins => {
                Ok(PermissionType::MessageAndLaunchOtherPlugins)
            },
            ProtobufPermissionType::Reconfigure => Ok(PermissionType::Reconfigure),
            ProtobufPermissionType::FullHdAccess => Ok(PermissionType::FullHdAccess),
            ProtobufPermissionType::StartWebServer => Ok(PermissionType::StartWebServer),
            ProtobufPermissionType::InterceptInput => Ok(PermissionType::InterceptInput),
        }
    }
}

impl TryFrom<PermissionType> for ProtobufPermissionType {
    type Error = &'static str;
    fn try_from(permission: PermissionType) -> Result<Self, &'static str> {
        match permission {
            PermissionType::ReadApplicationState => {
                Ok(ProtobufPermissionType::ReadApplicationState)
            },
            PermissionType::ChangeApplicationState => {
                Ok(ProtobufPermissionType::ChangeApplicationState)
            },
            PermissionType::OpenFiles => Ok(ProtobufPermissionType::OpenFiles),
            PermissionType::RunCommands => Ok(ProtobufPermissionType::RunCommands),
            PermissionType::OpenTerminalsOrPlugins => {
                Ok(ProtobufPermissionType::OpenTerminalsOrPlugins)
            },
            PermissionType::WriteToStdin => Ok(ProtobufPermissionType::WriteToStdin),
            PermissionType::WebAccess => Ok(ProtobufPermissionType::WebAccess),
            PermissionType::ReadCliPipes => Ok(ProtobufPermissionType::ReadCliPipes),
            PermissionType::MessageAndLaunchOtherPlugins => {
                Ok(ProtobufPermissionType::MessageAndLaunchOtherPlugins)
            },
            PermissionType::Reconfigure => Ok(ProtobufPermissionType::Reconfigure),
            PermissionType::FullHdAccess => Ok(ProtobufPermissionType::FullHdAccess),
            PermissionType::StartWebServer => Ok(ProtobufPermissionType::StartWebServer),
            PermissionType::InterceptInput => Ok(ProtobufPermissionType::InterceptInput),
        }
    }
}
