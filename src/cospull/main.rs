use log::{error};
use std::path::PathBuf;
mod api;
mod session;

async fn test()
{
    let mut cos = api::Cos::new(PathBuf::from("./test")).unwrap();
    if cos.login().await {
        cos.produce_by_page("life", 1).await;
    }else {
        error!("login error!");
    }
}

#[tokio::main]
async fn main() {
    let mut logger_builder = env_logger::Builder::from_default_env();
    logger_builder.target(env_logger::Target::Stdout);
    logger_builder.filter_level(log::LevelFilter::Info);
    logger_builder.init();
    test().await;
}
