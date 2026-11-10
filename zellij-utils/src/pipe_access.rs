//! Who may use zellij's named pipes on Windows.
//!
//! Windows gives a new pipe read access for every user and for anonymous logons. On Unix the
//! sockets sit in a folder only their user can open, but pipe names are machine-wide on Windows,
//! so each pipe carries its own access list instead. That folder also keeps other users from
//! putting a socket where a zellij client looks for one; on Windows anyone can create a pipe at a
//! name no zellij server holds yet, so clients check who owns the pipe they reached.

use std::io;
use std::os::windows::io::{AsHandle, AsRawHandle};
use std::ptr;

use interprocess::local_socket::Stream;
use interprocess::os::windows::security_descriptor::{
    AsSecurityDescriptorExt, BorrowedSecurityDescriptor, SecurityDescriptor,
};
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenIntegrityLevel, TokenOwner, TokenUser, OWNER_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_INFORMATION_CLASS, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Refuses a pipe that another user created.
pub fn check_owner(stream: &Stream) -> io::Result<()> {
    let owner = owner_of(handle_of(stream))?;
    if is_ours(&owner, &Identity::current()?) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("the pipe belongs to another user ({})", owner),
        ))
    }
}

/// A pipe made by this user, or by this user elevated when this process is elevated too.
fn is_ours(owner: &str, identity: &Identity) -> bool {
    owner == identity.user || owner == identity.owner
}

fn handle_of(stream: &Stream) -> HANDLE {
    match stream {
        Stream::NamedPipe(pipe) => pipe.inner().as_handle().as_raw_handle(),
    }
}

fn owner_of(handle: HANDLE) -> io::Result<String> {
    let mut owner = ptr::null_mut();
    let mut descriptor = ptr::null_mut();
    let error = unsafe {
        GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error as i32));
    }
    let sid = sid_string(owner);
    unsafe { LocalFree(descriptor) };
    sid
}

/// The security descriptor every zellij pipe listener is created with.
pub fn private_descriptor() -> io::Result<SecurityDescriptor> {
    let descriptor = Descriptor::private()?;
    unsafe { BorrowedSecurityDescriptor::from_ptr(descriptor.0) }.to_owned_sd()
}

/// Security attributes for a pipe created directly with `CreateNamedPipeW`.
pub struct PrivateAttributes(Descriptor);

impl PrivateAttributes {
    pub fn new() -> io::Result<Self> {
        Descriptor::private().map(PrivateAttributes)
    }

    /// Only valid while `self` is.
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0 .0,
            bInheritHandle: 0,
        }
    }
}

/// What Windows gives a new pipe, minus the read access for every user and for anonymous logons.
/// The label at this process's integrity level keeps processes below it from writing to the pipe,
/// so a normal process cannot drive a session started elevated. There is no owner or group: the
/// pipe takes them from the process, as without a descriptor.
fn private_sddl(identity: &Identity) -> String {
    format!(
        "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;{})S:(ML;;NW;;;{})",
        identity.owner, identity.level
    )
}

/// A security descriptor made from SDDL, which Windows allocates.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    fn private() -> io::Result<Self> {
        Self::from_sddl(&private_sddl(&Identity::current()?))
    }

    fn from_sddl(sddl: &str) -> io::Result<Self> {
        let sddl: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut descriptor = ptr::null_mut();
        let made = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        };
        if made == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Descriptor(descriptor))
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

/// Who this process runs as, as SID strings.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Identity {
    user: String,
    /// Who owns what this process creates: its user, or Administrators when it is elevated.
    owner: String,
    /// The integrity level, as its SID.
    level: String,
}

impl Identity {
    fn current() -> io::Result<Self> {
        let token = Token::open()?;
        Ok(Identity {
            user: token.sid(TokenUser)?,
            owner: token.sid(TokenOwner)?,
            level: token.sid(TokenIntegrityLevel)?,
        })
    }
}

struct Token(HANDLE);

impl Token {
    fn open() -> io::Result<Self> {
        let mut handle = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Token(handle))
    }

    /// The SID that `TOKEN_USER`, `TOKEN_OWNER` and `TOKEN_MANDATORY_LABEL` all start with.
    fn sid(&self, class: TOKEN_INFORMATION_CLASS) -> io::Result<String> {
        let mut length = 0;
        unsafe { GetTokenInformation(self.0, class, ptr::null_mut(), 0, &mut length) };
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        // u64s keep the pointers in the buffer aligned.
        let mut buffer = vec![0u64; (length as usize).div_ceil(8)];
        let read = unsafe {
            GetTokenInformation(
                self.0,
                class,
                buffer.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        };
        if read == 0 {
            return Err(io::Error::last_os_error());
        }
        sid_string(unsafe { *buffer.as_ptr().cast::<PSID>() })
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn sid_string(sid: PSID) -> io::Result<String> {
    let mut text = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let string = unsafe { wide_string(text) };
    unsafe { LocalFree(text.cast()) };
    Ok(string)
}

/// # Safety
/// `text` must point to a NUL-terminated UTF-16 string.
unsafe fn wide_string(text: *const u16) -> String {
    let length = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Security::Authorization::ConvertSecurityDescriptorToStringSecurityDescriptorW;
    use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, LABEL_SECURITY_INFORMATION};

    /// The access list and label of the pipe a client is connected to.
    fn access_of(stream: &Stream) -> String {
        let handle = handle_of(stream);
        let parts = DACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION;
        let mut descriptor = ptr::null_mut();
        let error = unsafe {
            GetSecurityInfo(
                handle,
                SE_KERNEL_OBJECT,
                parts,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        assert_eq!(error, 0, "GetSecurityInfo failed");
        let mut text = ptr::null_mut();
        let converted = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                parts,
                &mut text,
                ptr::null_mut(),
            )
        };
        assert_ne!(converted, 0, "{}", io::Error::last_os_error());
        let sddl = unsafe { wide_string(text) };
        unsafe {
            LocalFree(text.cast());
            LocalFree(descriptor);
        }
        sddl
    }

    fn assert_private(sddl: &str) {
        assert!(
            !sddl.contains(";;;WD)") && !sddl.contains(";;;AN)"),
            "every user or anonymous logons can open the pipe: {}",
            sddl
        );
        assert!(
            sddl.contains("(ML;;NW;;;"),
            "the pipe has no integrity label: {}",
            sddl
        );
    }

    #[test]
    fn a_session_pipe_and_its_reply_pipe_are_private() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("session");
        let _requests = crate::consts::ipc_bind(&path).unwrap();
        let _replies = crate::consts::ipc_bind_reply(&path).unwrap();
        assert_private(&access_of(&crate::consts::ipc_connect(&path).unwrap()));
        assert_private(&access_of(
            &crate::consts::ipc_connect_reply(&path).unwrap(),
        ));
    }

    #[test]
    fn an_async_pipe_is_private() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("web_server");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _listener = runtime.block_on(async { crate::consts::ipc_bind_async(&path).unwrap() });
        assert_private(&access_of(&crate::consts::ipc_connect(&path).unwrap()));
    }

    #[test]
    fn without_a_descriptor_windows_lets_every_user_read_a_pipe() {
        use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions};
        let dir = tempfile::TempDir::new().unwrap();
        let name = dir.path().join("default").to_string_lossy().to_string();
        let _listener = ListenerOptions::new()
            .name(name.as_str().to_ns_name::<GenericNamespaced>().unwrap())
            .create_sync()
            .unwrap();
        let stream =
            Stream::connect(name.as_str().to_ns_name::<GenericNamespaced>().unwrap()).unwrap();
        assert!(access_of(&stream).contains("(A;;FR;;;WD)"));
    }

    #[test]
    fn a_client_reaches_a_pipe_its_own_user_made() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("session");
        let _requests = crate::consts::ipc_bind(&path).unwrap();
        let _replies = crate::consts::ipc_bind_reply(&path).unwrap();
        crate::pipe_pairs::connect_pair(&path).unwrap();
    }

    #[test]
    fn a_pipe_that_refuses_this_user_fails_with_permission_denied() {
        use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions};
        use interprocess::os::windows::local_socket::ListenerOptionsExt;
        // Like a session started elevated, as seen by a normal process: only SYSTEM may use it.
        let only_system = Descriptor::from_sddl("D:P(A;;FA;;;SY)").unwrap();
        let descriptor = unsafe { BorrowedSecurityDescriptor::from_ptr(only_system.0) }
            .to_owned_sd()
            .unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("elevated");
        let name = path.to_string_lossy().to_string();
        let _listener = ListenerOptions::new()
            .name(name.as_str().to_ns_name::<GenericNamespaced>().unwrap())
            .security_descriptor(descriptor)
            .create_sync()
            .unwrap();
        let error = crate::pipe_pairs::connect_pair(&path).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied, "{}", error);
    }

    #[test]
    fn a_pipe_is_ours_when_our_user_or_our_elevated_self_made_it() {
        let user = "S-1-5-21-1-2-3-1001";
        let administrators = "S-1-5-32-544";
        let normal = Identity {
            user: user.to_owned(),
            owner: user.to_owned(),
            level: "S-1-16-8192".to_owned(),
        };
        let elevated = Identity {
            user: user.to_owned(),
            owner: administrators.to_owned(),
            level: "S-1-16-12288".to_owned(),
        };
        assert!(is_ours(user, &normal));
        assert!(is_ours(user, &elevated));
        assert!(is_ours(administrators, &elevated));
        assert!(
            !is_ours(administrators, &normal),
            "an elevated pipe is not a normal process's"
        );
        assert!(!is_ours("S-1-5-21-1-2-3-1002", &normal));
        assert!(!is_ours("S-1-5-21-1-2-3-1002", &elevated));
    }

    #[test]
    fn the_access_list_names_the_owner_and_integrity_level() {
        let identity = Identity {
            user: "S-1-5-21-1-2-3-1001".to_owned(),
            owner: "S-1-5-32-544".to_owned(),
            level: "S-1-16-12288".to_owned(),
        };
        assert_eq!(
            private_sddl(&identity),
            "D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;S-1-5-32-544)S:(ML;;NW;;;S-1-16-12288)"
        );
    }

    #[test]
    fn this_process_has_an_identity() {
        let identity = Identity::current().unwrap();
        for sid in [&identity.user, &identity.owner, &identity.level] {
            assert!(sid.starts_with("S-1-"), "{:?}", identity);
        }
        assert!(identity.level.starts_with("S-1-16-"), "{:?}", identity);
    }
}
