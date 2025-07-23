mod cmd_line;
mod linefeed_helper;
mod zmq_helper;

use core::sync::atomic::AtomicBool;

use anyhow::Result;
use clap::Parser;
use cmd_line::CommandLineOptions;
use linefeed_helper::run_terminal;
use tokio::{sync::mpsc::unbounded_channel, task, try_join};
use zmq_helper::run_zmq;

extern crate alloc;

pub(crate) static CONTINUE_RUNNING: AtomicBool = AtomicBool::new(true);

#[tokio::main(flavor = "multi_thread", worker_threads = 5)]
async fn main() -> Result<()> {
    let args = CommandLineOptions::parse();

    let (zmq_sender, zmq_receiver) = unbounded_channel();
    let (display_sender, display_receiver) = unbounded_channel();

    let cloned_args = args.clone();

    display_sender.send("Ctrl-C or 'exit' to exit rcon session".to_string())?;

    let zmq_task = task::spawn(run_zmq(cloned_args, zmq_receiver, display_sender));
    let terminal_task = task::spawn(run_terminal(args, zmq_sender, display_receiver));
    match try_join!(zmq_task, terminal_task)? {
        (Err(e), ..) | (.., Err(e)) => Err(e),
        _ => Ok(()),
    }
}
