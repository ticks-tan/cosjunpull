use log::{error};
use std::path::PathBuf;

mod api;
mod session;

async fn pull(tag: &str, output: &str)
{
    // 爬取文件输出目录
    let mut cos = api::Cos::new(PathBuf::from(output)).unwrap();
    if cos.login().await {
        cos.produce_by_page(tag, -1).await;
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

    // cospull <tag> output
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 {
        // 开始下载
        pull(
            args.get(1).unwrap().as_str(), 
            args.get(2).unwrap().as_str()
        ).await;
    }else {
        println!("cospull <tag> <target>");
    }
}
