pub use super::generated_api::api::plugin_permission::PermissionType as ProtobufPermissionType;
use crate::data::PermissionType;

use std::convert::TryFrom;

impl TryFrom<ProtobufPermissionType> for PermissionType {
    type Error = &'static str;
    fn try_from(protobuf_permission: ProtobufPermissionType) -> Result<Self, &'static str> {
        match protobuf_permission {
            ProtobufPermissionType::KeyboardInput => Ok(PermissionType::KeyboardInput),
        }
    }
}

impl TryFrom<PermissionType> for ProtobufPermissionType {
    type Error = &'static str;
    fn try_from(permission: PermissionType) -> Result<Self, &'static str> {
        match permission {
            PermissionType::KeyboardInput => Ok(ProtobufPermissionType::KeyboardInput),
        }
    }
}
