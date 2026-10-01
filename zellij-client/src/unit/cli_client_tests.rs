use crate::cli_client::{
    error_text, is_usage_error, plan_prompt, prompt_error_message, start_cli_client, PromptInput,
};
use crate::os_input_output::ClientOsApi;
use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use zellij_utils::cli::PromptCli;
use zellij_utils::data::Palette;
use zellij_utils::errors::ErrorContext;
use zellij_utils::input::actions::Action;
use zellij_utils::ipc::{ClientToServerMsg, ServerToClientMsg};
use zellij_utils::pane_size::Size;
use zellij_utils::prompt::PromptElement;

#[derive(Clone, Debug, Default)]
struct ScriptedOsApi {
    piped_stdin: Option<Vec<u8>>,
    replies: Arc<Mutex<VecDeque<Vec<ServerToClientMsg>>>>,
    inbox: Arc<Mutex<VecDeque<ServerToClientMsg>>>,
    sent_payloads: Arc<Mutex<Vec<Option<String>>>>,
    stdout: Arc<Mutex<Vec<u8>>>,
}

impl ScriptedOsApi {
    fn new(piped_stdin: Option<&str>, replies: Vec<Vec<ServerToClientMsg>>) -> Self {
        ScriptedOsApi {
            piped_stdin: piped_stdin.map(|s| s.as_bytes().to_vec()),
            replies: Arc::new(Mutex::new(replies.into_iter().collect())),
            ..Default::default()
        }
    }
    fn sent_payloads(&self) -> Vec<Option<String>> {
        self.sent_payloads.lock().unwrap().clone()
    }
    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.stdout.lock().unwrap()).into_owned()
    }
}

struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl ClientOsApi for ScriptedOsApi {
    fn get_terminal_size(&self) -> Size {
        Size { rows: 24, cols: 80 }
    }
    fn set_raw_mode(&mut self) {}
    fn unset_raw_mode(&self) -> Result<(), io::Error> {
        Ok(())
    }
    fn get_stdout_writer(&self) -> Box<dyn Write> {
        Box::new(SharedWriter(self.stdout.clone()))
    }
    fn get_stdin_reader(&self) -> Box<dyn io::BufRead> {
        Box::new(io::Cursor::new(
            self.piped_stdin.clone().unwrap_or_default(),
        ))
    }
    fn stdin_is_terminal(&self) -> bool {
        self.piped_stdin.is_none()
    }
    fn update_session_name(&mut self, _new_session_name: String) {}
    fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
        Ok(vec![])
    }
    fn box_clone(&self) -> Box<dyn ClientOsApi> {
        Box::new(self.clone())
    }
    fn send_to_server(&self, msg: ClientToServerMsg) {
        if let ClientToServerMsg::Action {
            action: Action::CliPipe { payload, .. },
            ..
        } = msg
        {
            self.sent_payloads.lock().unwrap().push(payload);
            if let Some(replies) = self.replies.lock().unwrap().pop_front() {
                self.inbox.lock().unwrap().extend(replies);
            }
        }
    }
    fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(msg) = self.inbox.lock().unwrap().pop_front() {
                return Some((msg, ErrorContext::default()));
            }
            if std::time::Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    fn handle_signals(
        &self,
        _sigwinch_cb: Box<dyn Fn()>,
        _quit_cb: Box<dyn Fn()>,
        _resize_receiver: Option<std::sync::mpsc::Receiver<()>>,
    ) {
    }
    fn connect_to_server(&self, _path: &std::path::Path) {}
    fn load_palette(&self) -> Palette {
        Palette::default()
    }
    fn enable_mouse(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn disable_mouse(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn pipe_action(payload: Option<&str>) -> Action {
    Action::CliPipe {
        pipe_id: "pipe".to_owned(),
        name: Some("name".to_owned()),
        payload: payload.map(|p| p.to_owned()),
        args: None,
        plugin: None,
        configuration: None,
        launch_new: false,
        skip_cache: false,
        floating: None,
        in_place: None,
        cwd: None,
        pane_title: None,
        popup: None,
        popup_no_focus: false,
    }
}

fn release(exit_code: Option<i32>) -> ServerToClientMsg {
    ServerToClientMsg::UnblockCliPipeInput {
        pipe_name: "pipe".to_owned(),
        exit_code,
    }
}

fn output(text: &str) -> ServerToClientMsg {
    ServerToClientMsg::CliPipeOutput {
        pipe_name: "pipe".to_owned(),
        output: text.to_owned(),
    }
}

fn run(os_api: &ScriptedOsApi, action: Action) -> i32 {
    start_cli_client(Box::new(os_api.clone()), "session", vec![action])
}

#[test]
fn piped_stdin_waits_for_the_release_after_the_input_ends() {
    let os_api = ScriptedOsApi::new(
        Some("a\nb\n"),
        vec![
            vec![release(None)],
            vec![release(None)],
            vec![output("answer\n"), release(None)],
        ],
    );
    assert_eq!(run(&os_api, pipe_action(None)), 0);
    assert_eq!(
        os_api.sent_payloads(),
        vec![Some("a\n".to_owned()), Some("b\n".to_owned()), None]
    );
    assert_eq!(os_api.stdout(), "answer\n");
}

#[test]
fn the_exit_code_of_the_release_is_passed_through() {
    let os_api = ScriptedOsApi::new(None, vec![vec![output("x\n"), release(Some(124))]]);
    assert_eq!(run(&os_api, pipe_action(None)), 124);
    assert_eq!(os_api.stdout(), "x\n");
}

#[test]
fn a_release_without_an_exit_code_still_exits_zero() {
    let os_api = ScriptedOsApi::new(None, vec![vec![release(None)]]);
    assert_eq!(run(&os_api, pipe_action(Some("payload"))), 0);
    assert_eq!(os_api.sent_payloads(), vec![Some("payload".to_owned())]);
}

#[test]
fn a_final_release_ends_piped_input_early() {
    let os_api = ScriptedOsApi::new(Some("a\nb\nc\n"), vec![vec![release(Some(1))]]);
    assert_eq!(run(&os_api, pipe_action(None)), 1);
    assert_eq!(os_api.sent_payloads(), vec![Some("a\n".to_owned())]);
}

#[test]
fn a_log_error_from_the_server_exits_two() {
    let os_api = ScriptedOsApi::new(
        None,
        vec![vec![ServerToClientMsg::LogError {
            lines: vec!["boom".to_owned()],
        }]],
    );
    assert_eq!(run(&os_api, pipe_action(None)), 2);
}

fn prompt_cli(args: &[&str]) -> PromptCli {
    let mut command_line = vec!["prompt"];
    command_line.extend_from_slice(args);
    <PromptCli as clap::Parser>::try_parse_from(command_line).unwrap()
}

fn no_stdin() -> Result<String, String> {
    Err("stdin should not be read".to_owned())
}

#[test]
fn prompt_options_become_pipe_arguments() {
    let plan = plan_prompt(
        &prompt_cli(&[
            "confirm",
            "Force push?",
            "--yes",
            "Push",
            "--no",
            "Cancel",
            "--timeout",
            "5s",
            "--json",
        ]),
        no_stdin,
        false,
    )
    .unwrap();
    assert_eq!(plan.element, PromptElement::Confirm);
    assert_eq!(plan.args.get("message").unwrap(), "Force push?");
    assert_eq!(plan.args.get("yes").unwrap(), "Push");
    assert_eq!(plan.args.get("no").unwrap(), "Cancel");
    assert_eq!(plan.args.get("timeout").unwrap(), "5s");
    assert_eq!(plan.args.get("json").unwrap(), "true");
    assert_eq!(plan.input, PromptInput::Single(None));
}

#[test]
fn choose_without_items_streams_stdin() {
    let plan = plan_prompt(&prompt_cli(&["choose", "-0", "--multi"]), no_stdin, true).unwrap();
    assert_eq!(plan.input, PromptInput::StreamStdin { null: true });
    assert!(plan.args.get("items").is_none());
    let plan = plan_prompt(
        &prompt_cli(&["choose", "a", "--item", "v=Label", "--selected", "a"]),
        no_stdin,
        true,
    )
    .unwrap();
    assert_eq!(plan.input, PromptInput::Single(None));
    assert_eq!(plan.args.get("items").unwrap(), r#"["a",["v","Label"]]"#);
    assert_eq!(plan.args.get("selected").unwrap(), r#"["a"]"#);
}

#[test]
fn bad_prompt_arguments_are_refused_before_connecting() {
    assert!(plan_prompt(&prompt_cli(&["choose"]), no_stdin, false).is_err());
    assert!(plan_prompt(&prompt_cli(&["input", "--validate", "("]), no_stdin, false).is_err());
    assert!(plan_prompt(
        &prompt_cli(&["number", "--min", "5", "--max", "1"]),
        no_stdin,
        false
    )
    .is_err());
    assert!(plan_prompt(
        &prompt_cli(&["confirm", "x", "--timeout", "soon"]),
        no_stdin,
        false
    )
    .is_err());
    assert!(plan_prompt(
        &prompt_cli(&["toggle", "CI", "--default", "maybe"]),
        no_stdin,
        false
    )
    .is_err());
    let error = plan_prompt(
        &prompt_cli(&[
            "number",
            "Port",
            "--min",
            "1",
            "--max",
            "10",
            "--default",
            "42",
        ]),
        no_stdin,
        false,
    )
    .unwrap_err();
    assert_eq!(error, "--default: 42 is not between 1 and 10");
    assert!(plan_prompt(
        &prompt_cli(&["select", "L", "a", "--default", "b"]),
        no_stdin,
        false
    )
    .is_err());
    let error = plan_prompt(
        &prompt_cli(&["form"]),
        || Ok(r#"{"fields":[{"id":"port","type":"number","min":"x"}]}"#.to_owned()),
        true,
    )
    .unwrap_err();
    assert!(error.contains("\"port\""), "{}", error);
}

#[test]
fn form_description_is_sent_as_the_payload() {
    let spec = r#"{"fields":[{"id":"name","type":"input"}]}"#;
    let plan = plan_prompt(&prompt_cli(&["form"]), || Ok(spec.to_owned()), true).unwrap();
    assert_eq!(plan.element, PromptElement::Form);
    assert_eq!(plan.input, PromptInput::Single(Some(spec.to_owned())));
}

#[test]
fn missing_required_arguments_are_usage_errors_that_show_the_help() {
    for args in [
        vec!["confirm"],
        vec!["toggle"],
        vec!["choose"],
        vec!["select"],
        vec!["select", "License"],
        vec!["menu"],
        vec!["form"],
    ] {
        let error = plan_prompt(&prompt_cli(&args), no_stdin, false).unwrap_err();
        assert!(is_usage_error(&error), "{:?}: {}", args, error);
    }
    let error =
        plan_prompt(&prompt_cli(&["input", "--validate", "("]), no_stdin, false).unwrap_err();
    assert!(!is_usage_error(&error));
    let help = zellij_utils::cli::prompt_element_help("confirm", false);
    assert!(help.contains("Usage: zellij prompt confirm"), "{}", help);
    assert!(help.contains("Examples:"), "{}", help);
    assert!(
        help.contains("zellij prompt confirm \"Force push?\""),
        "{}",
        help
    );
    assert_eq!(error_text(&error), error);
}

#[test]
fn a_missing_argument_prints_the_error_and_the_element_help() {
    let error = plan_prompt(&prompt_cli(&["toggle"]), no_stdin, false).unwrap_err();
    let message = prompt_error_message(&error, "toggle", false);
    assert!(
        message.starts_with("error: toggle: the label is missing\n\n"),
        "{}",
        message
    );
    assert!(
        message.contains("Usage: zellij prompt toggle"),
        "{}",
        message
    );
    assert!(message.contains("Examples:"), "{}", message);
    assert!(!message.contains('\u{1b}'), "{}", message);
}

#[test]
fn a_bad_value_prints_only_the_error() {
    let error = plan_prompt(
        &prompt_cli(&["number", "N", "--min", "5", "--max", "1"]),
        no_stdin,
        false,
    )
    .unwrap_err();
    let message = prompt_error_message(&error, "number", false);
    assert_eq!(message, "error: --min is larger than --max\n");
}

#[test]
fn the_help_is_coloured_for_a_terminal_and_plain_otherwise() {
    let coloured = zellij_utils::cli::prompt_element_help("choose", true);
    let plain = zellij_utils::cli::prompt_element_help("choose", false);
    assert!(coloured.contains("\u{1b}["), "{}", coloured);
    assert!(!plain.contains('\u{1b}'), "{}", plain);
    assert!(plain.contains("Examples:"));
}

#[test]
fn every_element_has_an_examples_section() {
    for element in [
        "confirm", "choose", "input", "number", "toggle", "select", "menu", "form", "notify",
    ] {
        let help = zellij_utils::cli::prompt_element_help(element, false);
        assert!(
            help.contains(&format!("Examples:\n  zellij prompt {}", element))
                || help.contains("Examples:\n  git branch")
                || help.contains("Examples:\n  echo"),
            "{}: {}",
            element,
            help
        );
    }
}

#[test]
fn bare_prompt_command_shows_the_help() {
    let result = std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            <zellij_utils::cli::CliArgs as clap::Parser>::try_parse_from(["zellij", "prompt"])
                .map(|_| ())
                .map_err(|e| (e.kind(), e.render().to_string()))
        })
        .unwrap()
        .join()
        .unwrap();
    let (kind, rendered) = result.unwrap_err();
    assert_eq!(
        kind,
        clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
    assert!(
        rendered.contains("confirm") && rendered.contains("choose"),
        "{}",
        rendered
    );
}

#[test]
fn placement_flags_choose_where_the_popup_opens() {
    use zellij_utils::data::PipePopupPlacement;
    assert_eq!(
        prompt_cli(&["toggle", "x"]).placement(),
        PipePopupPlacement::Pane
    );
    assert_eq!(
        prompt_cli(&["toggle", "x", "--at-center"]).placement(),
        PipePopupPlacement::Center
    );
    assert_eq!(
        prompt_cli(&["toggle", "x", "--at-mouse"]).placement(),
        PipePopupPlacement::Mouse
    );
    assert_eq!(
        prompt_cli(&["toggle", "x", "--at-cursor"]).placement(),
        PipePopupPlacement::Cursor
    );
    assert!(<PromptCli as clap::Parser>::try_parse_from([
        "prompt",
        "toggle",
        "x",
        "--at-mouse",
        "--at-cursor"
    ])
    .is_err());
}

#[test]
fn notify_defaults_to_the_top_right_corner_and_does_not_take_focus() {
    use zellij_utils::data::PipePopupPlacement;
    let notify = prompt_cli(&["notify", "Build finished"]);
    assert_eq!(notify.placement(), PipePopupPlacement::TopRight);
    assert!(!notify.takes_focus());
    assert_eq!(
        prompt_cli(&["notify", "x", "--at", "bottom-left"]).placement(),
        PipePopupPlacement::BottomLeft
    );
    assert!(prompt_cli(&["toggle", "x"]).takes_focus());
    assert!(<PromptCli as clap::Parser>::try_parse_from(["prompt", "notify", "x", "--at", "nowhere"]).is_err());
}

#[test]
fn notify_does_not_take_the_answer_and_placement_flags() {
    for flag in ["--json", "--at-cursor", "--at-mouse", "--at-center"] {
        assert!(
            <PromptCli as clap::Parser>::try_parse_from(["prompt", "notify", "x", flag]).is_err(),
            "{}",
            flag
        );
        assert!(
            <PromptCli as clap::Parser>::try_parse_from(["prompt", "toggle", "x", flag]).is_ok(),
            "{}",
            flag
        );
    }
    assert!(
        <PromptCli as clap::Parser>::try_parse_from(["prompt", "notify", "x", "--default", "y"])
            .is_err()
    );
    assert!(<PromptCli as clap::Parser>::try_parse_from([
        "prompt", "notify", "x", "--title", "t", "--timeout", "5s"
    ])
    .is_ok());
    let help = zellij_utils::cli::prompt_element_help("notify", false);
    for flag in ["--json", "--at-cursor", "--at-mouse", "--at-center", "--default"] {
        assert!(!help.contains(flag), "{}: {}", flag, help);
    }
    assert!(help.contains("--timeout") && help.contains("--at "));
}

#[test]
fn notify_needs_its_text_and_sends_it_as_the_message() {
    let plan = plan_prompt(&prompt_cli(&["notify", "Build finished", "--timeout", "5s"]), || Ok(String::new()), false).unwrap();
    assert_eq!(plan.element, zellij_utils::prompt::PromptElement::Notify);
    assert_eq!(
        plan.args.get(zellij_utils::prompt::ARG_MESSAGE).map(|m| m.as_str()),
        Some("Build finished")
    );
    assert!(!plan.args.contains_key(zellij_utils::prompt::ARG_NO_PANE_NAME));
    assert!(!plan.args.contains_key(zellij_utils::prompt::ARG_NO_TAB_NAME));
    let without_names = plan_prompt(
        &prompt_cli(&["notify", "x", "--no-pane-name", "--no-tab-name"]),
        || Ok(String::new()),
        false,
    )
    .unwrap();
    for arg in [
        zellij_utils::prompt::ARG_NO_PANE_NAME,
        zellij_utils::prompt::ARG_NO_TAB_NAME,
    ] {
        assert_eq!(without_names.args.get(arg).map(|m| m.as_str()), Some("true"));
    }
    assert!(plan_prompt(&prompt_cli(&["notify"]), || Ok(String::new()), false).is_err());
}

#[test]
fn queued_prompt_lines_are_sent_together_but_pipe_lines_are_not() {
    let mut queued: std::collections::VecDeque<Option<String>> =
        vec![Some("a\n".to_owned()), Some("b\n".to_owned()), None]
            .into_iter()
            .collect();
    assert_eq!(
        crate::cli_client::next_payload(&mut queued, true),
        Some(Some("a\nb\n".to_owned()))
    );
    assert_eq!(
        crate::cli_client::next_payload(&mut queued, true),
        Some(None)
    );
    let mut queued: std::collections::VecDeque<Option<String>> =
        vec![Some("a\n".to_owned()), Some("b\n".to_owned())]
            .into_iter()
            .collect();
    assert_eq!(
        crate::cli_client::next_payload(&mut queued, false),
        Some(Some("a\n".to_owned()))
    );
}
