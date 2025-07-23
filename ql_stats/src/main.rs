mod cmd_line;
mod zmq_helper;

use core::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use anyhow::Result;
use clap::Parser;
use cmd_line::CommandLineOptions;
use tokio::{
    sync::mpsc::{UnboundedReceiver, unbounded_channel},
    try_join,
};
use zmq_helper::run_zmq;

extern crate alloc;

pub(crate) static CONTINUE_RUNNING: AtomicBool = AtomicBool::new(true);

#[tokio::main(flavor = "multi_thread", worker_threads = 5)]
async fn main() -> Result<()> {
    let args = CommandLineOptions::parse();

    let (display_sender, display_receiver) = unbounded_channel();

    let cloned_args = args.clone();

    display_sender.send("Ctrl-C to exit zmq-stats session".to_string())?;

    let zmq_task = tokio::spawn(run_zmq(cloned_args, display_sender));
    let terminal_task = tokio::spawn(receive_json(display_receiver));
    match try_join!(zmq_task, terminal_task)? {
        (Err(e), ..) => Err(e),
        _ => Ok(()),
    }
}

async fn receive_json(mut receiver: UnboundedReceiver<String>) -> Result<()> {
    while CONTINUE_RUNNING.load(Ordering::Acquire) && !receiver.is_closed() {
        while let Ok(line) = receiver.try_recv() {
            println!("{line}");
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}
