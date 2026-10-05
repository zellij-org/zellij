//! The `[cli_client]` is used to attach to a running server session
//! and dispatch actions, that are specified through the command line.
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::io::{self, BufRead, Read, Write};
use std::process;
use std::str::FromStr;
use std::{fs, path::PathBuf};

use crate::os_input_output::ClientOsApi;
use uuid::Uuid;
use zellij_utils::{
    cli::{PromptCli, PromptElementCli, SubscribeCli, SubscribeFormat},
    data::{PaneId, PipePopupPlacement},
    errors::prelude::*,
    input::actions::Action,
    ipc::{ClientToServerMsg, ExitReason, ServerToClientMsg},
    prompt::{self, ChoiceItem, PromptElement},
};

pub fn start_cli_client(
    mut os_input: Box<dyn ClientOsApi>,
    session_name: &str,
    actions: Vec<Action>,
) -> i32 {
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };
    crate::check_ipc_pipe_length(&zellij_ipc_pipe);
    os_input.connect_to_server(&*zellij_ipc_pipe);
    let pane_id = os_input
        .env_variable("ZELLIJ_PANE_ID")
        .and_then(|e| e.trim().parse().ok());

    for action in actions {
        match action {
            Action::CliPipe {
                pipe_id,
                name,
                payload,
                plugin,
                args,
                configuration,
                launch_new,
                skip_cache,
                floating,
                in_place,
                cwd,
                pane_title,
                popup,
                popup_no_focus,
            } => {
                let exit_status = pipe_client(
                    &mut os_input,
                    PipeRequest {
                        pipe_id,
                        name,
                        plugin,
                        args,
                        configuration,
                        launch_new,
                        skip_cache,
                        floating,
                        in_place,
                        pane_id,
                        cwd,
                        pane_title,
                        popup,
                        popup_no_focus,
                    },
                    payload,
                );
                if exit_status != 0 {
                    os_input.send_to_server(ClientToServerMsg::ClientExited);
                    return exit_status;
                }
            },
            action => {
                if let Some(exit_status) =
                    individual_messages_client(&mut os_input, action, pane_id)
                {
                    return exit_status;
                }
            },
        }
    }
    os_input.send_to_server(ClientToServerMsg::ClientExited);
    0
}

struct PipeRequest {
    pipe_id: String,
    name: Option<String>,
    plugin: Option<String>,
    args: Option<BTreeMap<String, String>>,
    configuration: Option<BTreeMap<String, String>>,
    launch_new: bool,
    skip_cache: bool,
    floating: Option<bool>,
    in_place: Option<bool>,
    pane_id: Option<u32>,
    cwd: Option<PathBuf>,
    pane_title: Option<String>,
    popup: Option<PipePopupPlacement>,
    popup_no_focus: bool,
}

impl PipeRequest {
    fn prepare(mut self) -> Self {
        self.name = self
            .name
            .take()
            .or_else(|| self.plugin.clone())
            .or_else(|| Some(Uuid::new_v4().to_string()));
        if self.launch_new {
            self.configuration
                .get_or_insert_with(BTreeMap::new)
                .insert("_zellij_id".to_owned(), Uuid::new_v4().to_string());
        }
        self
    }
    fn message(&self, payload: Option<String>) -> ClientToServerMsg {
        ClientToServerMsg::Action {
            action: Action::CliPipe {
                pipe_id: self.pipe_id.clone(),
                name: self.name.clone(),
                payload,
                args: self.args.clone(),
                plugin: self.plugin.clone(),
                configuration: self.configuration.clone(),
                floating: self.floating,
                in_place: self.in_place,
                launch_new: self.launch_new,
                skip_cache: self.skip_cache,
                cwd: self.cwd.clone(),
                pane_title: self.pane_title.clone(),
                popup: self.popup,
                popup_no_focus: self.popup_no_focus,
            },
            terminal_id: self.pane_id,
            client_id: None,
            is_cli_client: true,
        }
    }
}

enum PipeWaitOutcome {
    Released(Option<i32>),
    Exit(i32),
}

fn handle_pipe_server_message(
    os_input: &mut Box<dyn ClientOsApi>,
    pipe_id: &str,
    message: ServerToClientMsg,
) -> Option<PipeWaitOutcome> {
    match message {
        ServerToClientMsg::UnblockCliPipeInput {
            pipe_name,
            exit_code,
        } => {
            if pipe_name == pipe_id {
                Some(PipeWaitOutcome::Released(exit_code))
            } else {
                None
            }
        },
        ServerToClientMsg::CliPipeOutput { pipe_name, output } => {
            let err_context = "Failed to write to stdout";
            if pipe_name == pipe_id {
                let mut stdout = os_input.get_stdout_writer();
                stdout
                    .write_all(output.as_bytes())
                    .context(err_context)
                    .non_fatal();
                stdout.flush().context(err_context).non_fatal();
            }
            None
        },
        ServerToClientMsg::Log { lines: log_lines } => {
            log_lines.iter().for_each(|line| println!("{line}"));
            Some(PipeWaitOutcome::Exit(0))
        },
        ServerToClientMsg::LogError { lines: log_lines } => {
            log_lines.iter().for_each(|line| eprintln!("{line}"));
            Some(PipeWaitOutcome::Exit(2))
        },
        ServerToClientMsg::Exit { exit_reason } => match exit_reason {
            ExitReason::Error(e) => {
                eprintln!("{}", e);
                Some(PipeWaitOutcome::Exit(2))
            },
            ExitReason::CustomExitStatus(exit_status) => Some(PipeWaitOutcome::Exit(exit_status)),
            _ => Some(PipeWaitOutcome::Exit(0)),
        },
        _ => None,
    }
}

fn wait_for_pipe_release(os_input: &mut Box<dyn ClientOsApi>, pipe_id: &str) -> PipeWaitOutcome {
    loop {
        match os_input.recv_from_server() {
            Some((message, _)) => {
                if let Some(outcome) = handle_pipe_server_message(os_input, pipe_id, message) {
                    return outcome;
                }
            },
            None => return PipeWaitOutcome::Exit(2),
        }
    }
}

enum PipeEvent {
    Server(ServerToClientMsg),
    ServerGone,
    Payload(Option<String>),
}

const STREAM_BATCH_BYTES: usize = 256 * 1024;

pub(crate) fn next_payload(
    queued: &mut VecDeque<Option<String>>,
    batch: bool,
) -> Option<Option<String>> {
    let first = queued.pop_front()?;
    let Some(mut payload) = first else {
        return Some(None);
    };
    if batch {
        while let Some(Some(next)) = queued.front() {
            if payload.len() + next.len() > STREAM_BATCH_BYTES {
                break;
            }
            payload.push_str(next);
            queued.pop_front();
        }
    }
    Some(Some(payload))
}

fn drive_streaming_pipe(
    os_input: &mut Box<dyn ClientOsApi>,
    request: &PipeRequest,
    first_payload: Option<String>,
    separator: u8,
    batch: bool,
) -> i32 {
    let (sender, receiver) = std::sync::mpsc::channel();
    {
        let sender = sender.clone();
        let stdin_os_input = os_input.box_clone();
        std::thread::spawn(move || {
            if let Some(payload) = first_payload {
                if sender.send(PipeEvent::Payload(Some(payload))).is_err() {
                    return;
                }
            }
            let payloads = StdinPayloads {
                reader: stdin_os_input.get_stdin_reader(),
                separator,
                ended: false,
            };
            for payload in payloads {
                if sender.send(PipeEvent::Payload(payload)).is_err() {
                    return;
                }
            }
        });
    }
    {
        let server_os_input = os_input.box_clone();
        std::thread::spawn(move || loop {
            match server_os_input.recv_from_server() {
                Some((message, _)) => {
                    if sender.send(PipeEvent::Server(message)).is_err() {
                        return;
                    }
                },
                None => {
                    let _ = sender.send(PipeEvent::ServerGone);
                    return;
                },
            }
        });
    }
    let mut queued: VecDeque<Option<String>> = VecDeque::new();
    let mut waiting_for_release = false;
    let mut input_ended = false;
    loop {
        match receiver.recv() {
            Ok(PipeEvent::Payload(payload)) => {
                if payload.is_none() {
                    input_ended = true;
                }
                queued.push_back(payload);
            },
            Ok(PipeEvent::Server(message)) => {
                match handle_pipe_server_message(os_input, &request.pipe_id, message) {
                    Some(PipeWaitOutcome::Released(Some(exit_code))) => return exit_code,
                    Some(PipeWaitOutcome::Released(None)) => {
                        waiting_for_release = false;
                        if input_ended && queued.is_empty() {
                            return 0;
                        }
                    },
                    Some(PipeWaitOutcome::Exit(exit_code)) => return exit_code,
                    None => {},
                }
            },
            Ok(PipeEvent::ServerGone) | Err(_) => return 2,
        }
        if !waiting_for_release {
            if let Some(payload) = next_payload(&mut queued, batch) {
                os_input.send_to_server(request.message(payload));
                waiting_for_release = true;
            }
        }
    }
}

fn drive_pipe(
    os_input: &mut Box<dyn ClientOsApi>,
    request: &PipeRequest,
    mut payloads: impl Iterator<Item = Option<String>>,
) -> i32 {
    let mut next_payload = payloads.next();
    while let Some(payload) = next_payload {
        os_input.send_to_server(request.message(payload));
        match wait_for_pipe_release(os_input, &request.pipe_id) {
            PipeWaitOutcome::Released(Some(exit_code)) => return exit_code,
            PipeWaitOutcome::Released(None) => {},
            PipeWaitOutcome::Exit(exit_code) => return exit_code,
        }
        next_payload = payloads.next();
    }
    0
}

struct StdinPayloads {
    reader: Box<dyn BufRead>,
    separator: u8,
    ended: bool,
}

impl Iterator for StdinPayloads {
    type Item = Option<String>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.ended {
            return None;
        }
        let mut buffer = vec![];
        let _ = self.reader.read_until(self.separator, &mut buffer);
        if buffer.is_empty() {
            self.ended = true;
            return Some(None);
        }
        Some(Some(String::from_utf8_lossy(&buffer).into_owned()))
    }
}

fn pipe_client(
    os_input: &mut Box<dyn ClientOsApi>,
    request: PipeRequest,
    payload: Option<String>,
) -> i32 {
    let request = request.prepare();
    let is_piped = !os_input.stdin_is_terminal();
    if !is_piped {
        return drive_pipe(os_input, &request, std::iter::once(payload));
    }
    drive_streaming_pipe(os_input, &request, payload, b'\n', false)
}

pub fn start_prompt_client(
    mut os_input: Box<dyn ClientOsApi>,
    session_name: &str,
    prompt_cli: PromptCli,
    plugin_url: String,
) -> i32 {
    let is_piped = !os_input.stdin_is_terminal();
    let plan = match plan_prompt(
        &prompt_cli,
        || {
            let mut text = String::new();
            os_input
                .get_stdin_reader()
                .read_to_string(&mut text)
                .map(|_| text)
                .map_err(|e| format!("failed to read stdin: {}", e))
        },
        is_piped,
    ) {
        Ok(plan) => plan,
        Err(e) => {
            eprint!(
                "{}",
                prompt_error_message(
                    &e,
                    prompt_cli.element.name(),
                    std::io::IsTerminal::is_terminal(&std::io::stderr()),
                )
            );
            return prompt::EXIT_ERROR;
        },
    };
    let pane_id: Option<u32> = os_input
        .env_variable("ZELLIJ_PANE_ID")
        .and_then(|e| e.trim().parse().ok());
    let Some(pane_id) = pane_id else {
        eprintln!("zellij prompt must be run inside a Zellij pane (ZELLIJ_PANE_ID is not set)");
        return prompt::EXIT_ERROR;
    };
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };
    crate::check_ipc_pipe_length(&zellij_ipc_pipe);
    os_input.connect_to_server(&*zellij_ipc_pipe);
    let request = PipeRequest {
        pipe_id: Uuid::new_v4().to_string(),
        name: Some(plan.element.name().to_owned()),
        plugin: Some(plugin_url),
        args: Some(plan.args),
        configuration: None,
        launch_new: true,
        skip_cache: false,
        floating: None,
        in_place: None,
        pane_id: Some(pane_id),
        cwd: None,
        pane_title: Some(format!("prompt: {}", plan.element.name())),
        popup: Some(prompt_cli.placement()),
        popup_no_focus: !prompt_cli.takes_focus(),
    }
    .prepare();
    let exit_status = match plan.input {
        PromptInput::Single(payload) => {
            drive_pipe(&mut os_input, &request, std::iter::once(payload))
        },
        PromptInput::StreamStdin { null } => drive_streaming_pipe(
            &mut os_input,
            &request,
            None,
            if null { 0 } else { b'\n' },
            true,
        ),
    };
    os_input.send_to_server(ClientToServerMsg::ClientExited);
    exit_status
}

#[derive(Debug, Clone, PartialEq)]
pub enum PromptInput {
    Single(Option<String>),
    StreamStdin { null: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PromptPlan {
    pub element: PromptElement,
    pub args: BTreeMap<String, String>,
    pub input: PromptInput,
}

const USAGE_ERROR_MARKER: &str = "\u{0}usage";

fn usage_error(message: &str) -> String {
    format!("{}{}", USAGE_ERROR_MARKER, message)
}

pub fn is_usage_error(error: &str) -> bool {
    error.starts_with(USAGE_ERROR_MARKER)
}

pub fn error_text(error: &str) -> &str {
    error.strip_prefix(USAGE_ERROR_MARKER).unwrap_or(error)
}

pub fn prompt_error_message(error: &str, element: &str, color: bool) -> String {
    let mut message = format!("error: {}\n", error_text(error));
    if is_usage_error(error) {
        message.push('\n');
        message.push_str(&zellij_utils::cli::prompt_element_help(element, color));
    }
    message
}

fn check_pattern(pattern: &str) -> Result<(), String> {
    regex::Regex::new(pattern)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn plan_prompt(
    prompt_cli: &PromptCli,
    read_stdin: impl FnOnce() -> Result<String, String>,
    stdin_is_piped: bool,
) -> Result<PromptPlan, String> {
    let mut args = BTreeMap::new();
    let answer_args = prompt_cli.answer_args();
    if let Some(title) = &prompt_cli.title {
        args.insert(prompt::ARG_TITLE.to_owned(), title.clone());
    }
    if let Some(timeout) = &prompt_cli.timeout {
        prompt::parse_duration(timeout).map_err(|e| format!("--timeout: {}", e))?;
        args.insert(prompt::ARG_TIMEOUT.to_owned(), timeout.clone());
    }
    if let Some(default) = &answer_args.default {
        args.insert(prompt::ARG_DEFAULT.to_owned(), default.clone());
    }
    if answer_args.json {
        args.insert(prompt::ARG_JSON.to_owned(), "true".to_owned());
    }
    let mut input = PromptInput::Single(None);
    let element = match &prompt_cli.element {
        PromptElementCli::Confirm {
            message, yes, no, ..
        } => {
            let message = message
                .as_ref()
                .ok_or_else(|| usage_error("confirm: the question is missing"))?;
            args.insert(prompt::ARG_MESSAGE.to_owned(), message.clone());
            if let Some(yes) = yes {
                args.insert(prompt::ARG_YES.to_owned(), yes.clone());
            }
            if let Some(no) = no {
                args.insert(prompt::ARG_NO.to_owned(), no.clone());
            }
            if let Some(default) = &answer_args.default {
                prompt::parse_bool(default).map_err(|e| format!("--default: {}", e))?;
            }
            PromptElement::Confirm
        },
        PromptElementCli::Choose {
            items,
            item,
            multi,
            labels,
            null,
            selected,
            ..
        } => {
            let mut choices: Vec<ChoiceItem> =
                items.iter().map(|i| ChoiceItem::plain(i.clone())).collect();
            choices.extend(item.iter().map(|i| ChoiceItem::from_item_arg(i)));
            if choices.is_empty() {
                if !stdin_is_piped {
                    return Err(usage_error(
                        "choose: no items given; pass them as arguments or pipe them into stdin",
                    ));
                }
                input = PromptInput::StreamStdin { null: *null };
            } else {
                args.insert(prompt::ARG_ITEMS.to_owned(), prompt::encode_items(&choices));
            }
            if *multi {
                args.insert(prompt::ARG_MULTI.to_owned(), "true".to_owned());
            }
            if *labels {
                args.insert(prompt::ARG_LABELS.to_owned(), "true".to_owned());
            }
            if *null {
                args.insert(prompt::ARG_NULL.to_owned(), "true".to_owned());
            }
            if !selected.is_empty() {
                args.insert(
                    prompt::ARG_SELECTED.to_owned(),
                    prompt::encode_list(selected),
                );
            }
            PromptElement::Choose
        },
        PromptElementCli::Input {
            message,
            placeholder,
            validate,
            required,
            ..
        } => {
            if let Some(message) = message {
                args.insert(prompt::ARG_MESSAGE.to_owned(), message.clone());
            }
            if let Some(placeholder) = placeholder {
                args.insert(prompt::ARG_PLACEHOLDER.to_owned(), placeholder.clone());
            }
            if let Some(validate) = validate {
                check_pattern(validate).map_err(|e| format!("--validate: {}", e))?;
                args.insert(prompt::ARG_VALIDATE.to_owned(), validate.clone());
            }
            if *required {
                args.insert(prompt::ARG_REQUIRED.to_owned(), "true".to_owned());
            }
            PromptElement::Input
        },
        PromptElementCli::Number {
            message,
            min,
            max,
            step,
            ..
        } => {
            if let Some(message) = message {
                args.insert(prompt::ARG_MESSAGE.to_owned(), message.clone());
            }
            if let (Some(min), Some(max)) = (min, max) {
                if min > max {
                    return Err("--min is larger than --max".to_owned());
                }
            }
            if let Some(step) = step {
                if *step <= 0 {
                    return Err("--step must be larger than 0".to_owned());
                }
                args.insert(prompt::ARG_STEP.to_owned(), step.to_string());
            }
            if let Some(min) = min {
                args.insert(prompt::ARG_MIN.to_owned(), min.to_string());
            }
            if let Some(max) = max {
                args.insert(prompt::ARG_MAX.to_owned(), max.to_string());
            }
            if let Some(default) = &answer_args.default {
                let value = default
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| format!("--default: '{}' is not a whole number", default))?;
                let below = min.map(|min| value < min).unwrap_or(false);
                let above = max.map(|max| value > max).unwrap_or(false);
                if below || above {
                    let range = match (min, max) {
                        (Some(min), Some(max)) => format!("between {} and {}", min, max),
                        (Some(min), None) => format!("at least {}", min),
                        (_, Some(max)) => format!("at most {}", max),
                        _ => String::new(),
                    };
                    return Err(format!("--default: {} is not {}", value, range));
                }
            }
            PromptElement::Number
        },
        PromptElementCli::Toggle { message, .. } => {
            let message = message
                .as_ref()
                .ok_or_else(|| usage_error("toggle: the label is missing"))?;
            args.insert(prompt::ARG_MESSAGE.to_owned(), message.clone());
            if let Some(default) = &answer_args.default {
                prompt::parse_bool(default).map_err(|e| format!("--default: {}", e))?;
            }
            PromptElement::Toggle
        },
        PromptElementCli::Select {
            message, options, ..
        } => {
            if options.is_empty() {
                return Err(usage_error("select: give a label followed by the options"));
            }
            if let Some(message) = message {
                args.insert(prompt::ARG_MESSAGE.to_owned(), message.clone());
            }
            if let Some(default) = &answer_args.default {
                if !options.contains(default) {
                    return Err(format!(
                        "--default: '{}' is not one of the options",
                        default
                    ));
                }
            }
            args.insert(prompt::ARG_OPTIONS.to_owned(), prompt::encode_list(options));
            PromptElement::Select
        },
        PromptElementCli::Menu { items, item, .. } => {
            let mut choices: Vec<ChoiceItem> =
                items.iter().map(|i| ChoiceItem::plain(i.clone())).collect();
            choices.extend(item.iter().map(|i| ChoiceItem::from_item_arg(i)));
            if choices.is_empty() {
                return Err(usage_error("menu: no items given"));
            }
            args.insert(prompt::ARG_ITEMS.to_owned(), prompt::encode_items(&choices));
            PromptElement::Menu
        },
        PromptElementCli::Form { spec, .. } => {
            let text = match spec {
                Some(path) => fs::read_to_string(path)
                    .map_err(|e| format!("failed to read {}: {}", path.display(), e))?,
                None => {
                    if !stdin_is_piped {
                        return Err(usage_error(
                            "form: pass the form description with --spec or on stdin",
                        ));
                    }
                    read_stdin()?
                },
            };
            let form_spec = prompt::parse_form_spec(&text)?;
            prompt::check_form_patterns(&form_spec, check_pattern)?;
            if let Some(default) = &answer_args.default {
                match serde_json::from_str::<serde_json::Value>(default) {
                    Ok(serde_json::Value::Object(_)) => {},
                    _ => return Err("--default: must be a JSON object for form".to_owned()),
                }
            }
            input = PromptInput::Single(Some(text));
            PromptElement::Form
        },
        PromptElementCli::Notify {
            message,
            no_pane_name,
            no_tab_name,
            ..
        } => {
            let message = message
                .as_ref()
                .ok_or_else(|| usage_error("notify: the text is missing"))?;
            args.insert(prompt::ARG_MESSAGE.to_owned(), message.clone());
            if *no_pane_name {
                args.insert(prompt::ARG_NO_PANE_NAME.to_owned(), "true".to_owned());
            }
            if *no_tab_name {
                args.insert(prompt::ARG_NO_TAB_NAME.to_owned(), "true".to_owned());
            }
            PromptElement::Notify
        },
    };
    Ok(PromptPlan {
        element,
        args,
        input,
    })
}

fn individual_messages_client(
    os_input: &mut Box<dyn ClientOsApi>,
    action: Action,
    pane_id: Option<u32>,
) -> Option<i32> {
    let is_blocking = matches!(
        &action,
        Action::NewBlockingPane {
            unblock_condition: Some(_),
            ..
        }
    );
    let msg = ClientToServerMsg::Action {
        action,
        terminal_id: pane_id,
        client_id: None,
        is_cli_client: true,
    };
    os_input.send_to_server(msg);
    loop {
        match os_input.recv_from_server() {
            Some((ServerToClientMsg::UnblockInputThread, _)) if !is_blocking => {
                return None;
            },
            Some((ServerToClientMsg::Log { lines: log_lines }, _)) => {
                log_lines.iter().for_each(|line| println!("{line}"));
                return None;
            },
            Some((ServerToClientMsg::LogError { lines: log_lines }, _)) => {
                log_lines.iter().for_each(|line| eprintln!("{line}"));
                return Some(2);
            },
            Some((ServerToClientMsg::Exit { exit_reason }, _)) => match exit_reason {
                ExitReason::Error(e) => {
                    eprintln!("{}", e);
                    return Some(2);
                },
                ExitReason::CustomExitStatus(exit_status) => {
                    return Some(exit_status);
                },
                _ => {
                    return None;
                },
            },
            _ => {},
        }
    }
}

pub fn start_subscribe_client(
    os_input: Box<dyn ClientOsApi>,
    session_name: &str,
    subscribe_cli: SubscribeCli,
) {
    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };
    crate::check_ipc_pipe_length(&zellij_ipc_pipe);
    os_input.connect_to_server(&*zellij_ipc_pipe);

    // Parse pane IDs
    let pane_ids: Vec<PaneId> = subscribe_cli
        .pane_id
        .iter()
        .map(|s| {
            PaneId::from_str(s).unwrap_or_else(|e| {
                eprintln!("Invalid pane ID '{}': {}", s, e);
                process::exit(2);
            })
        })
        .collect();

    // Send subscribe message
    os_input.send_to_server(ClientToServerMsg::SubscribeToPaneRenders {
        pane_ids: pane_ids.clone(),
        scrollback: subscribe_cli.scrollback,
        ansi: subscribe_cli.ansi,
    });

    // Track remaining panes for exit-on-all-closed
    let mut remaining_panes: HashSet<PaneId> = pane_ids.into_iter().collect();

    // Streaming receive loop
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    loop {
        match os_input.recv_from_server() {
            Some((
                ServerToClientMsg::PaneRenderUpdate {
                    pane_id,
                    viewport,
                    scrollback,
                    is_initial,
                },
                _,
            )) => match subscribe_cli.format {
                SubscribeFormat::Raw => {
                    if let Some(ref scrollback_lines) = scrollback {
                        for line in scrollback_lines {
                            let _ = writeln!(stdout, "{}", line);
                        }
                    }
                    for line in &viewport {
                        let _ = writeln!(stdout, "{}", line);
                    }
                    let _ = stdout.flush();
                },
                SubscribeFormat::Json => {
                    let json = serde_json::json!({
                        "event": "pane_update",
                        "pane_id": pane_id.to_string(),
                        "viewport": viewport,
                        "scrollback": scrollback,
                        "is_initial": is_initial,
                    });
                    let _ = writeln!(stdout, "{}", json);
                    let _ = stdout.flush();
                },
            },
            Some((ServerToClientMsg::SubscribedPaneClosed { pane_id }, _)) => {
                remaining_panes.remove(&pane_id);
                match subscribe_cli.format {
                    SubscribeFormat::Raw => {},
                    SubscribeFormat::Json => {
                        let json = serde_json::json!({
                            "event": "pane_closed",
                            "pane_id": pane_id.to_string(),
                        });
                        let _ = writeln!(stdout, "{}", json);
                        let _ = stdout.flush();
                    },
                }
                if remaining_panes.is_empty() {
                    break;
                }
            },
            Some((ServerToClientMsg::Exit { .. }, _)) => break,
            Some((ServerToClientMsg::LogError { lines }, _)) => {
                for line in lines {
                    eprintln!("{}", line);
                }
                process::exit(2);
            },
            None => break,
            _ => {},
        }
    }

    os_input.send_to_server(ClientToServerMsg::ClientExited);
}
