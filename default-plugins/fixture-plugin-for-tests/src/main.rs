use serde::{Deserialize, Serialize};
use zellij_tile::prelude::*;

// This is a fixture plugin used only for tests in Zellij
// it is not (and should not!) be included in the mainline executable
// it's included here for convenience so that it will be built by the CI

#[derive(Default)]
struct State {
    received_events: Vec<Event>,
    received_payload: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct TestWorker {
    number_of_messages_received: usize,
}

impl<'de> ZellijWorker<'de> for TestWorker {
    fn on_message(&mut self, message: String, payload: String) {
        if message == "ping" {
            self.number_of_messages_received += 1;
            post_message_to_plugin(
                "pong".into(),
                format!(
                    "{}, received {} messages",
                    payload, self.number_of_messages_received
                ),
            );
        }
    }
}

register_plugin!(State);
register_worker!(TestWorker, test_worker, TEST_WORKER);

impl ZellijPlugin for State {
    fn load(&mut self) {
        subscribe(&[
            EventType::InputReceived,
            EventType::SystemClipboardFailure,
            EventType::CustomMessage,
            EventType::FileSystemCreate,
            EventType::FileSystemRead,
            EventType::FileSystemUpdate,
            EventType::FileSystemDelete,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match &event {
            Event::CustomMessage(message, payload) => {
                if message == "pong" {
                    self.received_payload = Some(payload.clone());
                }
            },
            Event::SystemClipboardFailure => {
                // this is just to trigger the worker message
                post_message_to(
                    "test",
                    "ping".to_owned(),
                    "gimme_back_my_payload".to_owned(),
                );
            },
            _ => {},
        }
        let should_render = true;
        self.received_events.push(event);
        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if let Some(payload) = self.received_payload.as_ref() {
            println!("Payload from worker: {:?}", payload);
        } else {
            println!(
                "Rows: {:?}, Cols: {:?}, Received events: {:?}",
                rows, cols, self.received_events
            );
        }
    }
}
