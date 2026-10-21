use anyhow::{Context, Result};
use windows::core::{Interface, HSTRING};
use windows::Win32::Storage::EnhancedStorage::PKEY_Title;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{
    DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, ShellLink,
};

use crate::com::{set_string, Com};
use crate::identity::identity;
use crate::launcher::window_command;

pub struct Task {
    pub title: &'static str,
    pub arguments: &'static str,
}

pub const TASKS: [Task; 3] = [
    Task {
        title: "Show Inbox",
        arguments: "--inbox",
    },
    Task {
        title: "Show Sessions",
        arguments: "--sessions",
    },
    Task {
        title: "New Window",
        arguments: "",
    },
];

pub fn install() {
    if let Err(e) = try_install() {
        report!("the taskbar shortcuts could not be set up: {:#}", e);
    } else {
        let _ = zellij_utils::desktop_integration::update(|state| {
            state.windows.jump_list = true;
        });
    }
}

fn try_install() -> Result<()> {
    let _com = Com::initialize()?;
    let exe = std::env::current_exe().context("failed to locate the running binary")?;
    let command = window_command(&exe);
    unsafe {
        let list: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)
                .context("failed to reach the jump list")?;
        list.SetAppID(&HSTRING::from(identity().windows_app_id.as_str()))?;
        let mut slots = 0u32;
        let _removed: IObjectArray = list.BeginList(&mut slots)?;
        let tasks: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)
                .context("failed to create the task list")?;
        for task in TASKS {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .context("failed to create a shell link")?;
            link.SetPath(&HSTRING::from(command.program.as_path()))?;
            let arguments = if command.uses_launcher() {
                task.arguments.to_owned()
            } else {
                format!("{} {}", command.arguments, task.arguments)
                    .trim()
                    .to_owned()
            };
            link.SetArguments(&HSTRING::from(arguments))?;
            link.SetIconLocation(&HSTRING::from(exe.as_path()), 0)?;
            let store: IPropertyStore = link.cast()?;
            set_string(&store, &PKEY_Title, task.title)?;
            store.Commit()?;
            tasks.AddObject(&link)?;
        }
        let array: IObjectArray = tasks.cast()?;
        list.AddUserTasks(&array)?;
        list.CommitList()?;
    }
    Ok(())
}
