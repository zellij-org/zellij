use crate::{
    os_input_output::{AsyncReader, ServerOsApi},
    screen::ScreenInstruction,
    thread_bus::ThreadSenders,
};
use async_std::{
    future::timeout as async_timeout,
    task,
};
use std::{
    os::unix::io::RawFd,
    time::{Duration, Instant},
};
use zellij_utils::{
    async_std,
    errors::{get_current_ctx, ContextType},
    logging::debug_to_file,
};

enum ReadResult {
    Ok(usize),
    Timeout,
    Err(std::io::Error),
}

impl From<std::io::Result<usize>> for ReadResult {
    fn from(e: std::io::Result<usize>) -> ReadResult {
        match e {
            Err(e) => ReadResult::Err(e),
            Ok(n) => ReadResult::Ok(n),
        }
    }
}

pub(crate) struct TerminalBytes {
    pid: RawFd,
    senders: ThreadSenders,
    async_reader: Box<dyn AsyncReader>,
    debug: bool,
    render_deadline: Option<Instant>,
    backed_up: bool,
    minimum_render_send_time: Option<Duration>,
    buffering_pause: Duration,
    last_render: Instant
}

impl TerminalBytes {
    pub fn new(pid: RawFd, senders: ThreadSenders, os_input: Box<dyn ServerOsApi>, debug: bool) -> Self {
        TerminalBytes {
            pid,
            senders,
            debug,
            async_reader: os_input.async_file_reader(pid),
            render_deadline: None,
            backed_up: false,
            minimum_render_send_time: None,
            buffering_pause: Duration::from_millis(30),
            last_render: Instant::now(),
        }
    }
    pub async fn listen(&mut self) {
        // This function reads bytes from the pty and then sends them as
        // ScreenInstruction::PtyBytes to screen to be parsed there
        // We also send a separate instruction to Screen to render as ScreenInstruction::Render
        //
        // We endeavour to send a Render instruction to screen immediately after having send bytes
        // to parse - this is so that the rendering is quick and smooth. However, this can cause
        // latency if the screen is backed up. For this reason, if we detect a peak in the time it
        // takes to send the render instruction, we assume the screen thread is backed up and so
        // only send a render instruction sparingly, giving screen time to process bytes and render
        // while still allowing the user to see an indication that things are happening (the
        // sparing render instructions)
        let mut err_ctx = get_current_ctx();
        err_ctx.add_call(ContextType::AsyncTask);
        let mut buf = [0u8; 65536];
        loop {
            match self.deadline_read(&mut buf).await {
            // match deadline_read(async_reader.as_mut(), self.render_deadline, &mut buf).await {
                ReadResult::Ok(0) | ReadResult::Err(_) => break, // EOF or error
                ReadResult::Timeout => {
                    let time_to_send_render = self.async_send_to_screen(ScreenInstruction::Render).await;
                    self.update_render_send_time(time_to_send_render);
                    // next read does not need a deadline as we just rendered everything
                    self.render_deadline = None;
                    self.last_render = Instant::now();
                },
                ReadResult::Ok(n_bytes) => {
                    let bytes = &buf[..n_bytes];
                    if self.debug {
                        let _ = debug_to_file(bytes, self.pid);
                    }
                    self.async_send_to_screen(ScreenInstruction::PtyBytes(self.pid, bytes.to_vec())).await;
                    if !self.backed_up {
                        // we're not backed up, let's send an immediate render instruction
                        let time_to_send_render = self.async_send_to_screen(ScreenInstruction::Render).await;
                        self.update_render_send_time(time_to_send_render);
                    }
                    // if we already have a render_deadline we keep it, otherwise we set it
                    // to buffering_pause since the last time we rendered.
                    self.render_deadline.get_or_insert(self.last_render + self.buffering_pause);
                },
            }
        }
        self.async_send_to_screen(ScreenInstruction::Render).await;
    }
    async fn async_send_to_screen(&self, screen_instruction: ScreenInstruction) -> Duration { // returns the time it blocked the thread for
        let sent_at = Instant::now();
        let senders = self.senders.clone();
        task::spawn_blocking(move || senders.send_to_screen(screen_instruction))
            .await
            .unwrap();
        sent_at.elapsed()
    }
    fn update_render_send_time(&mut self, time_to_send_render: Duration) {
        match self.minimum_render_send_time.as_mut() {
            Some(minimum_render_time) => {
                if time_to_send_render < *minimum_render_time {
                    *minimum_render_time = time_to_send_render;
                }
                if time_to_send_render > *minimum_render_time * 10 {
                    // sending the render instruction took an especially long time, we can safely
                    // assume the screen thread is backed up and we should only send render
                    // instructions sparingly
                    self.backed_up = true;
                } else if time_to_send_render < *minimum_render_time * 5 {
                    // the screen thread is not backed up, we atomically unset the backed_up
                    // indication
                    self.backed_up = false;
                }
            },
            None => {
                self.minimum_render_send_time = Some(time_to_send_render);
            }
        }
    }
    async fn deadline_read(
        &mut self,
        buf: &mut [u8],
    ) -> ReadResult {
        if let Some(deadline) = self.render_deadline {
            let timeout = deadline.checked_duration_since(Instant::now());
            if let Some(timeout) = timeout {
                match async_timeout(timeout, self.async_reader.read(buf)).await {
                    Ok(res) => res.into(),
                    _ => ReadResult::Timeout,
                }
            } else {
                // deadline has already elapsed
                ReadResult::Timeout
            }
        } else {
            self.async_reader.read(buf).await.into()
        }
    }
}
