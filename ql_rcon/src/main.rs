//! Remote console for Quake Live servers: sends the lines typed in the terminal to the ZMQ rcon interface and prints the
//! replies with the Quake color codes rendered.
mod cmd_line;
mod linefeed_helper;
mod zmq_helper;

use core::sync::atomic::AtomicBool;

use anyhow::Result;
use clap::Parser;
use cmd_line::CommandLineOptions;
use linefeed_helper::run_terminal;
use tokio::{sync::mpsc::unbounded_channel, try_join};
use zmq_helper::run_zmq;

extern crate alloc;

pub(crate) static CONTINUE_RUNNING: AtomicBool = AtomicBool::new(true);

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = CommandLineOptions::parse();

    let (zmq_sender, zmq_receiver) = unbounded_channel();
    let (display_sender, display_receiver) = unbounded_channel();

    let cloned_args = args.clone();

    display_sender.send("Ctrl-C or 'exit' to exit rcon session".to_string())?;

    try_join!(
        run_zmq(cloned_args, zmq_receiver, display_sender),
        run_terminal(args, zmq_sender, display_receiver)
    )?;

    Ok(())
}
