/// Macro for testing round-trip conversion for ClientToServerMsg variants
macro_rules! test_client_roundtrip {
    ($msg:expr) => {{
        let original: crate::ipc::ClientToServerMsg = $msg;
        let proto: crate::client_server_contract::client_server_contract::ClientToServerMsg =
            original.clone().into();
        let roundtrip: crate::ipc::ClientToServerMsg = proto
            .try_into()
            .expect("Failed to convert back from protobuf");
        assert_eq!(original, roundtrip);
    }};
}

/// Macro for testing round-trip conversion for ServerToClientMsg variants
macro_rules! test_server_roundtrip {
    ($msg:expr) => {{
        let original: crate::ipc::ServerToClientMsg = $msg;
        let proto: crate::client_server_contract::client_server_contract::ServerToClientMsg =
            original.clone().into();
        let roundtrip: crate::ipc::ServerToClientMsg = proto
            .try_into()
            .expect("Failed to convert back from protobuf");
        assert_eq!(original, roundtrip);
    }};
}

pub(crate) use {test_client_roundtrip, test_server_roundtrip};
