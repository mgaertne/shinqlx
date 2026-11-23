//! Prints the game statistics that a Quake Live server publishes over its ZMQ stats interface.
mod cmd_line;
mod format;
mod zmq_helper;

use core::sync::atomic::AtomicBool;

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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = CommandLineOptions::parse();

    let (display_sender, display_receiver) = unbounded_channel();

    let cloned_args = args.clone();

    display_sender.send("Ctrl-C to exit zmq-stats session".to_string())?;

    try_join!(
        run_zmq(cloned_args, display_sender),
        receive_json(display_receiver)
    )?;

    Ok(())
}

async fn receive_json(mut receiver: UnboundedReceiver<String>) -> Result<()> {
    while let Some(line) = receiver.recv().await {
        println!("{line}");
    }

    Ok(())
}

#[cfg(test)]
mod main_tests {
    use tokio::sync::mpsc::unbounded_channel;

    use super::receive_json;

    #[tokio::test]
    async fn receive_json_ends_when_the_sender_is_dropped() {
        let (sender, receiver) = unbounded_channel();
        sender.send("line".to_string()).expect("receiver is alive");
        drop(sender);

        assert!(receive_json(receiver).await.is_ok());
    }
}
