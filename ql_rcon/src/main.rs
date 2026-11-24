//! Remote console for Quake Live servers: sends the lines typed in the terminal to the ZMQ rcon interface and prints the
//! replies with the Quake color codes rendered.
mod cmd_line;
mod fetch;
mod guide;
mod learn;
mod line_editor;
mod profile;
mod zmq_helper;

use anyhow::Result;
use line_editor::run_terminal;
use tokio::{sync::mpsc::unbounded_channel, try_join};
use tokio_util::sync::CancellationToken;
use zmq_helper::run_zmq;

extern crate alloc;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = cmd_line::parse()?;

    let (zmq_sender, zmq_receiver) = unbounded_channel();
    let (display_sender, display_receiver) = unbounded_channel();

    let cloned_args = args.clone();
    let running = CancellationToken::new();

    display_sender.send("Ctrl-C or 'exit' to exit rcon session".to_string())?;

    try_join!(
        run_zmq(cloned_args, zmq_receiver, display_sender, &running),
        run_terminal(args, zmq_sender, display_receiver, &running)
    )?;

    Ok(())
}
