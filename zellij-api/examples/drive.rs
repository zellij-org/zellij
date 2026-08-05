//! A small client for the Zellij remote-control API — useful both as a live
//! demo and for debugging the event stream.
//!
//! ```sh
//! target/dev-opt/zellij api-server --token secret &
//! cargo run -p zellij-api --example drive -- --token secret --session demo
//! ```
//!
//! It creates (or reuses) a session, subscribes to every pane, types a command
//! and prints every frame the server sends, so you can watch the screen diffs
//! arrive.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() {
    let mut port = 8787u16;
    let mut token: Option<String> = None;
    let mut session = "demo".to_string();
    let mut text = "echo hello_from_the_api\n".to_string();
    let mut watch_secs = 8u64;
    let mut exec: Vec<Value> = Vec::new();

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                port = args[i + 1].parse().expect("bad --port");
                i += 2;
            },
            "--token" => {
                token = Some(args[i + 1].clone());
                i += 2;
            },
            "--session" => {
                session = args[i + 1].clone();
                i += 2;
            },
            "--text" => {
                text = args[i + 1].clone();
                i += 2;
            },
            "--watch" => {
                watch_secs = args[i + 1].parse().expect("bad --watch");
                i += 2;
            },
            // Send a raw JSON command instead of the scripted scenario. May be
            // repeated; commands run in order.
            "--exec" => {
                exec.push(
                    serde_json::from_str::<Value>(&args[i + 1]).expect("--exec must be JSON"),
                );
                i += 2;
            },
            other => {
                eprintln!("unknown argument: {}", other);
                std::process::exit(2);
            },
        }
    }

    let url = match &token {
        Some(token) => format!("ws://127.0.0.1:{}/api?token={}", port, token),
        None => format!("ws://127.0.0.1:{}/api", port),
    };
    let (mut socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("could not connect — is `zellij api-server` running?");

    let mut next_id = 1u64;
    let mut call = |command: Value| {
        let id = next_id.to_string();
        next_id += 1;
        let mut command = command;
        command["id"] = json!(id);
        (id, command)
    };

    // Helper that sends a command and prints frames until its reply arrives.
    macro_rules! run {
        ($cmd:expr) => {{
            let (id, command) = call($cmd);
            println!("\x1b[36m-> {}\x1b[0m", command);
            socket
                .send(Message::Text(command.to_string().into()))
                .await
                .expect("send failed");
            loop {
                let Some(Ok(Message::Text(raw))) = socket.next().await else {
                    panic!("socket closed");
                };
                let value: Value = serde_json::from_str(&raw).expect("not JSON");
                if value["id"] == json!(id) {
                    println!("\x1b[32m<- {}\x1b[0m", value);
                    break value;
                }
                print_event(&value);
            }
        }};
    }

    if !exec.is_empty() {
        for command in exec.clone() {
            run!(command);
        }
        println!("\n--- watching events for {}s ---\n", watch_secs);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(watch_secs);
        while let Ok(Some(Ok(Message::Text(raw)))) =
            tokio::time::timeout_at(deadline, socket.next()).await
        {
            if let Ok(value) = serde_json::from_str::<Value>(&raw) {
                print_event(&value);
            }
        }
        return;
    }

    let sessions = run!(json!({"cmd": "session.list"}));
    let existing: Vec<&str> = sessions["result"]["sessions"]
        .as_array()
        .map(|s| {
            s.iter()
                .filter(|s| s["resurrectable"] == json!(false))
                .filter_map(|s| s["name"].as_str())
                .collect()
        })
        .unwrap_or_default();

    if !existing.contains(&session.as_str()) {
        run!(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}));
    }

    run!(json!({"cmd": "tab.list", "session": session}));
    run!(json!({"cmd": "pane.list", "session": session}));
    run!(json!({"cmd": "screen.subscribe", "session": session}));
    run!(json!({"cmd": "input.text", "session": session, "text": text}));

    println!("\n--- watching events for {}s ---\n", watch_secs);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(watch_secs);
    loop {
        match tokio::time::timeout_at(deadline, socket.next()).await {
            Ok(Some(Ok(Message::Text(raw)))) => {
                let value: Value = serde_json::from_str(&raw).expect("not JSON");
                print_event(&value);
            },
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(e))) => {
                eprintln!("websocket error: {}", e);
                break;
            },
            Ok(None) | Err(_) => break,
        }
    }
}

fn print_event(value: &Value) {
    match value["event"].as_str() {
        Some("screen.diff") => {
            println!(
                "\x1b[33m[screen.diff] pane={} seq={} +{} -{}\x1b[0m",
                value["pane_id"], value["seq"], value["added"], value["removed"]
            );
            for line in value["unified"].as_str().unwrap_or("").lines() {
                let colour = match line.chars().next() {
                    Some('+') => "\x1b[32m",
                    Some('-') => "\x1b[31m",
                    Some('@') => "\x1b[36m",
                    _ => "\x1b[90m",
                };
                println!("  {}{}\x1b[0m", colour, line);
            }
        },
        Some("screen.reset") => println!(
            "\x1b[35m[screen.reset] pane={} seq={} ({} lines)\x1b[0m",
            value["pane_id"],
            value["seq"],
            value["lines"].as_array().map(|l| l.len()).unwrap_or(0)
        ),
        Some(other) => println!("\x1b[90m[{}] {}\x1b[0m", other, value),
        None => println!("\x1b[90m{}\x1b[0m", value),
    }
}
