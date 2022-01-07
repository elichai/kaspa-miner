#![cfg_attr(all(test, feature = "bench"), feature(test))]

use std::env::consts::DLL_EXTENSION;
use std::env::current_exe;
use std::error::Error as StdError;
use std::ffi::OsStr;

use std::fs;
use log::{info, warn};
use clap::{App,IntoApp,FromArgMatches};
use kaspa_miner::PluginManager;

use crate::cli::Opt;
use crate::client::KaspadHandler;
use crate::miner::MinerManager;
use crate::proto::NotifyBlockAddedRequestMessage;
use crate::target::Uint256;

mod cli;
mod client;
mod kaspad_messages;
mod miner;
mod pow;
mod target;
mod watch;

const WHITELIST: [&str; 2] = ["libkaspacuda", "libkaspaopencl"];

pub mod proto {
    tonic::include_proto!("protowire");
    // include!("protowire.rs"); // FIXME: https://github.com/intellij-rust/intellij-rust/issues/6579
}

pub type Error = Box<dyn StdError + Send + Sync + 'static>;

type Hash = Uint256;

fn filter_plugins(dirname: &str) -> Vec<String>{
    match fs::read_dir(dirname) {
        Ok(readdir) => readdir.map(
            |entry| entry.unwrap().path()
        ).filter(
            |fname|
                fname.is_file() && fname.extension().is_some() && fname.extension().and_then(OsStr::to_str).unwrap_or_default().starts_with(DLL_EXTENSION)
        ).filter(
            |fname| WHITELIST.iter().find(|lib| **lib == fname.file_stem().and_then(OsStr::to_str).unwrap()).is_some()
        ).map(|path| path.to_str().unwrap().to_string()).collect::<Vec<String>>(),
        _ => Vec::<String>::new()
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut path = current_exe().unwrap_or_default();
    path.pop(); // Getting the parent directory
    let plugins = filter_plugins(path.to_str().unwrap_or("."));
    let (app, mut plugin_manager): (App, PluginManager) = kaspa_miner::load_plugins(Opt::into_app().term_width(120), &plugins)?;

    let matches = app.get_matches();

    plugin_manager.process_options(&matches);
    let mut opt: Opt = Opt::from_arg_matches(&matches)?;
    opt.process()?;
    env_logger::builder().filter_level(opt.log_level()).parse_default_env().init();
    info!("Found plugins: {:?}", plugins);

    loop {
        let mut client =
            KaspadHandler::connect(opt.kaspad_address.clone(), opt.mining_address.clone(), opt.mine_when_not_synced)
                .await?;
        if opt.devfund_percent > 0 {
            client.add_devfund( opt.devfund_address.clone(), opt.devfund_percent);
            info!(
                "devfund enabled, mining {}.{}% of the time to devfund address: {} ",
                opt.devfund_percent / 100,
                opt.devfund_percent % 100,
                opt.devfund_address
            );
        }
        client.client_send(NotifyBlockAddedRequestMessage {}).await?;
        client.client_get_block_template().await?;
        let mut miner_manager = MinerManager::new(client.send_channel.clone(), opt.num_threads, &plugin_manager);
        client.listen(&mut miner_manager).await?;
        warn!("Disconnected from kaspad, retrying");
    }
}
