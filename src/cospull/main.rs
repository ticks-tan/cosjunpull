use log::{error};
use std::path::PathBuf;

mod api;
mod session;

async fn test()
{
    // 爬取文件输出目录
    let mut cos = api::Cos::new(PathBuf::from("./target/out")).unwrap();
    if cos.login().await {
        cos.produce_by_page("life", 1).await;
    }else {
        error!("login error! More infomation: https://www.cosjun.cn");
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
