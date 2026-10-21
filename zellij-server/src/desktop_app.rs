use std::sync::OnceLock;

use zellij_utils::app_control::{send_app_request, AppReply, AppRequest, OpenWindowRequest};
use zellij_utils::desktop_integration::platform;

pub type AppStarter = Box<dyn Fn(&OpenWindowRequest) -> Result<(), String> + Send + Sync>;

static APP_STARTER: OnceLock<AppStarter> = OnceLock::new();

pub fn set_app_starter(starter: AppStarter) {
    let _ = APP_STARTER.set(starter);
}

pub fn open_window(request: OpenWindowRequest) -> Result<(), String> {
    match send_app_request(&AppRequest::OpenWindow(request.clone())) {
        Ok(AppReply::Refused(reason)) => Err(reason),
        Ok(_) => Ok(()),
        Err(_) => start_app_with(&request),
    }
}

fn start_app_with(request: &OpenWindowRequest) -> Result<(), String> {
    match APP_STARTER.get() {
        Some(start) => start(request),
        None => Err("The desktop app is not running and cannot be started from here".to_owned()),
    }
}

pub fn desktop_integration(request: &str) -> String {
    platform::handle(request)
}
