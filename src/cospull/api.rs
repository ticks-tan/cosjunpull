use std::{ collections::VecDeque };
use tokio::{task::JoinHandle};
use visdom::Vis;
use std::path::PathBuf;
use std::io::Write;
use log::{error, warn, info};
use crate::session::{self};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

// 创建目录
pub async fn create_dir(path: &PathBuf) -> bool {
    if path.exists() {
        return true;
    }
    match tokio::fs::create_dir_all(path).await {
        Ok(_) => true,
        Err(e) => {
            error!("create dir: {} error: {}", path.display(), e);
            false
        }
    }
}

// 下载文件
pub async fn download_files(bar: ProgressBar, tp: &str, folder: PathBuf, mut vec: VecDeque<String>) {
    if !create_dir(&folder).await {
        error!("create dir: {} error!", &folder.display().to_string());
        return ;
    }
    let mut info = match std::fs::File::create(folder.join("info.txt"))
            .map(std::io::BufWriter::new) {
        Ok(f) => Some(f),
        Err(_) => None
    };

    warn!("start download {} {} in dir: {}", vec.len(), tp, &folder.display());
    let mut i = 1;
    while !vec.is_empty() {
        let url = vec.pop_front().unwrap();
        if let Some(f) = &mut info {
            let _ = writeln!(f, "{}", &url);
        }
        bar.set_message(format!("downloading {} #{}", tp, i));
        bar.inc(1);
        i += 1;
    }
    if let Some(f) = &mut info {
        let _ = f.flush();
    }
    bar.finish_with_message("Down!");
}


// 生产器 -> 获取总体页数 -> 获取当前处理页数内所有项目并加入链表
// 消费器 -> 从链表获取头部连接 -> 初始化本地文件夹 -> 请求并下载图片和视频

pub struct CosItem
{
    // 标题(对应本地文件夹名称)
    pub title: String,
    // 项目 URL
    pub url: String
}

pub struct Cos
{
    // Http Client
    http_request: session::Session,
    // 保存文件夹
    folder: PathBuf
}

impl CosItem {
    fn new(title: String, url: String) -> Self {
        Self { title, url}
    }
}

impl Cos {

    pub fn new(folder: PathBuf) -> anyhow::Result<Self> {
        let path = PathBuf::from("./session.json");
        let session = session::Session::try_new(path).map_err(|e|{
            anyhow::anyhow!("{}", e).context("Cos create session error!")
        })?;
        Ok(Self {
            http_request: session,
            folder,
        })
    }

    // 初始化所有页数
    async fn init_total_page(self: &mut Self, tag: &str, total_page: &mut i32) -> bool {
        let res = self.http_request
            .http_get(&format!("https://www.cosjun.cn/{}?ref=cosjun", tag)).await;
        match res {
            Ok(res) => {
                let html = res.text().await;
                match html {
                    Ok(html) => {
                        let html = Vis::load(html).unwrap();
                        let node1 = html.find(".numeric-pagination")
                                                    .find(".page-numbers")
                                                    .find(":nth-last-child(2)");
                        match node1.text().parse::<i32>() {
                            Ok(i) => *total_page = i,
                            Err(e) => {
                                error!("<{}> ==> init_total_page parse number error: {}", tag, e);
                                *total_page = -1;
                                return false;
                            }
                        }
                    },
                    Err(e) => {
                        error!("<{}> ==> init_total_page parse response text error: {}", tag, e);
                        return false;
                    }
                }
            },
            Err(e) => {
                error!("<{}> ==> init_total_page get http request error:{}", tag, e);
                return false;
            }
        }
        return true;
    }

    // 获取每页所有项目
    pub async fn produce_by_page(self: &mut Self, tag: &str, max_page: i32) {
        let mut total_page = -1;
        let re = self.init_total_page(tag, &mut total_page).await;
        if !re {
            return;
        }
        let mut item_list: VecDeque<CosItem> = VecDeque::with_capacity(total_page as usize);
        info!("<{}> ==> max page/total page == {}/{}", tag, total_page, max_page);
        // 当前处理页数
        let mut cur_index = 1;
        while cur_index <= total_page && (max_page == -1 || cur_index <= max_page) {
            let get_url : String = format!("https://www.cosjun.cn/{}/page/{}?ref=cosjun", tag, cur_index);
            info!("<{}> ==> current page: {} ==> {}", tag, cur_index, &get_url);
            let res = self.http_request.http_get(&get_url).await;
            match res {
                Ok(res) => {
                    let html = res.text().await;
                    match html {
                        Ok(html) => {
                            match Vis::load(html) {
                                Ok(html) => {
                                    let node = html.find(".entry-wrapper")
                                                            .find(".entry-title")
                                                            .find("a");
                                    // println!("node: {}", node.htmls().as_str());
                                    node.into_iter().for_each(|item|{
                                        let title = item.get_attribute("title");
                                        let url = item.get_attribute("href");
                                        if let Some(title) = title {
                                            if let Some(url) = url {
                                                // 添加到队列
                                                // debug!("benzi => Found [{}] -> {}", title.to_string(), url.to_string());
                                                item_list.push_back(
                                                    CosItem::new(title.to_string(), url.to_string())
                                                );
                                            }
                                        }
                                    });
                                },
                                Err(e) => {
                                    error!("<{}> => item_produce parse html error: {}", tag, e);
                                }
                            }
                        },
                        Err(e) => {
                            error!("<{}> => item_produce get response text error: {}", tag, e);
                        }
                    }
                },
                Err(e) => {
                    error!("<{}> => item_produce get http request error: {}", tag, e);
                }
            }
            cur_index += 1;
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }
        self.item_process(&mut item_list, tag).await;
    }

    async fn item_process(self: &mut Self, item_list: &mut VecDeque<CosItem>, tag: &str) {
        while !item_list.is_empty() {
            let item = item_list.pop_front().unwrap();
            // 初始化目录
            let dir = self.folder.join(tag).join(&item.title);
            if dir.exists() {
                warn!("dir {} is exist, skip this page!", &item.title);
                continue;
            }
            let res = self.http_request.http_get(&item.url).await;
            match res {
                Ok(res) => {
                    match res.text().await {
                        Ok(html) => {
                            match Vis::load(html) {
                                Ok(html) => {
                                    let mut imgs_vec: VecDeque<String> = VecDeque::new();
                                    let mut video_vec: VecDeque<String> = VecDeque::new();
                                    // 查找图片
                                    let imgs = html.find(".gallery-icon > a");
                                    imgs.into_iter().for_each(|item|{
                                        if let Some(img) = item.get_attribute("href") {
                                            imgs_vec.push_back(img.to_string());
                                        }
                                    });
                                    // 查找视频
                                    let videos = html.find("video > a");
                                    videos.into_iter().for_each(|item|{
                                        if let Some(video) = item.get_attribute("href") {
                                            video_vec.push_back(video.to_string());
                                        }
                                    });
                                    let dir1 = dir.join("videos");

                                    let m = MultiProgress::new();
                                    let sty = ProgressStyle::with_template(
                                        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
                                        ).unwrap().progress_chars("##-");
                                    let mut h1 : Option<JoinHandle<_>> = None;
                                    let mut h2 : Option<JoinHandle<_>> = None;
                                    let mut pb : Option<ProgressBar> = None;
                                    let mut pb1 : Option<ProgressBar> = None;
                                    if !imgs_vec.is_empty() {
                                        pb  = Some(m.add(ProgressBar::new(imgs_vec.len() as u64)));
                                        pb.as_ref().unwrap().set_style(sty.clone());
                                    }
                                    if !video_vec.is_empty() {
                                        if let Some(pb) = &pb {
                                            pb1 = Some(m.insert_after(pb, ProgressBar::new(video_vec.len() as u64)));
                                        }else {
                                            pb1 = Some(m.add(ProgressBar::new(video_vec.len() as u64)));
                                        }
                                        pb1.as_ref().unwrap().set_style(sty);
                                    }
                                    if let Some(pb) = pb {
                                        h1 = Some(tokio::spawn(async move {
                                            download_files(pb, "img", dir.join("imgs"), imgs_vec).await;
                                        }));
                                    }
                                    if let Some(pb) = pb1 {
                                        h2 = Some(tokio::spawn(async move {
                                            download_files(pb, "video", dir1, video_vec).await;
                                        }));
                                    }
                                    if let Some(h) = h1 {
                                        let _ = h.await;
                                    }
                                    if let Some(h) = h2 {
                                        let _ = h.await;
                                    }
                                    
                                },
                                Err(e) => {
                                    error!("<{}> ==> item_process parse html error: {}", tag, e);
                                }
                            }
                        },
                        Err(e) => {
                            error!("<{}> ==> item_process get response text error: {}", tag, e);
                        }
                    }
                },
                Err(e) => {
                    error!("<{}> ==> item_process get http request error: {}", tag, e);
                }
            }
            // 延时
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }

    // 登陆获取session
    #[allow(dead_code)]
    pub async fn login(self: &mut Self) ->bool {
        self.http_request.login("<username>", "<password>").await
    }

    #[allow(dead_code)]
    pub async fn logout(self: &mut Self) {
        self.http_request.logout().await
    }

}