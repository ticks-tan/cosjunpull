use walkdir::WalkDir;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use log::{error, info, warn};

fn create_dirs(dir: &PathBuf) ->bool {
    if dir.exists() {
        return true;
    }
    match std::fs::create_dir_all(dir) {
        Ok(_) => true,
        Err(e) => {
            error!("create dir error: {}", e.to_string());
            false
        }
    }
}

struct Dup
{
    // 已下载完成目录
    downloaded_vec: Vec<PathBuf>,
    // 下载完成后压缩目录
    zip_path: PathBuf,
    // 最多下载几个目录
    chunk_size: usize,
}

impl Dup {
    fn new(zip_path: PathBuf, chunk_size: usize) -> Self {
        Self { downloaded_vec: Vec::with_capacity(40), zip_path, chunk_size }
    }

    fn start_download(self: &mut Self, root_dir: &str) {
        let mut start_index = 0;
        let mut end_index =0;
        for entry in WalkDir::new(root_dir)
                .into_iter() {
            if let Err(_) = entry {
                continue;
            }
            let entry = entry.unwrap();
            if entry.file_name() != "info.txt" {
                continue;
            }
            let entry = entry.into_path();
            info!("find info.txt in : {}", entry.to_str().unwrap());
            // 查找一级父目录
            if let Some(p1) = entry.parent() {
                if p1.exists() && p1.is_dir() {
                    let p1 = p1.to_path_buf();
                    // 查找二级父目录
                    if let Some(p2) = p1.parent() {
                        if Dup::download(p1.clone()) {
                            self.downloaded_vec.push(p2.to_path_buf());
                            end_index += 1;
                            if self.downloaded_vec.len() >= self.chunk_size {
                                let filename = format!("cos_{}_{}-{}.tar.gz", 
                                    PathBuf::from(root_dir).file_name().unwrap().to_str().unwrap(),
                                    start_index, end_index);
                                if self.compress_downloaded(&filename) {
                                    end_index += 1;
                                    start_index = end_index;
                                }
                            }
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
        let filename = format!("cos_{}_{}-{}.tar.gz", 
            PathBuf::from(root_dir).file_name().unwrap().to_str().unwrap(),
            start_index, end_index
        );
        self.compress_downloaded(&filename);
    }

    fn download(path: PathBuf) -> bool {
        info!("start download files in dir: {}", path.to_str().unwrap());
        let cmd = format!("cd \"{}\" && wget -nc -c -t 5 -T 120 -i \"info.txt\"", path.to_str().unwrap());
        // info!("run command: {}", &cmd);
        match Command::new("/bin/sh")
            .arg("-c")
            .arg(&cmd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn() {
                Ok(mut child) => {
                    match child.wait() {
                        Ok(status) => status.success(),
                        Err(_) => false
                    }
                },
                Err(e) => {
                    error!("run command {} error: {}", &cmd, e.to_string());
                    false
                }
            }
    }

    fn compress_downloaded(self: &mut Self, filename: &str) -> bool {
        info!("start compress files in dir: {}", self.zip_path.join(filename).to_str().unwrap());
        let mut cmd = format!("tar zcf \"{}\"", self.zip_path.join(filename).to_str().unwrap());
        for dir in &self.downloaded_vec {
            cmd += &format!(" \"{}\"", dir.to_str().unwrap());
        }
        let mut result = create_dirs(&self.zip_path);
        if result {
            result =  match Command::new("/bin/sh")
                .arg("-c")
                .arg(&cmd)
                .stdout(Stdio::null())
                .spawn() {
                    Ok(mut child) => {
                        match child.wait() {
                            Ok(status) => status.success(),
                            Err(_) => false
                        }
                    },
                    Err(e) => {
                        error!("run command {} error: {}", &cmd, e.to_string());
                        false
                    }
            };
        }
        if result {
            info!("compress files success, rm src files");
            for it in &self.downloaded_vec {
                let _ = std::fs::remove_dir_all(it);
            }
            if self.upload(self.zip_path.join(filename)) {
                info!("upload file: {} success!", self.zip_path.join(filename).to_str().unwrap());
            }
        }else {
            warn!("compress file error!");
        }
        self.downloaded_vec.clear();
        result
    }

    fn upload(self: &Self, path: PathBuf) -> bool {
        // use https://github.com/aoaostar/alidrive-uploader
        let cmd = format!("alidrive -c ./alidrive.yaml {} CosJun/zips", 
            path.to_str().unwrap()
        );
        info!("upload file use: {}", &cmd);
        // info!("run command: {}", &cmd);
        match Command::new("/bin/sh")
            .arg("-c")
            .arg(&cmd)
            .stdout(Stdio::null())
            .spawn() {
                Ok(mut child) => {
                    match child.wait() {
                        Ok(status) => status.success(),
                        Err(_) => false
                    }
                },
                Err(e) => {
                    error!("run command {} error: {}", &cmd, e.to_string());
                    false
                }
            }
    }
}

fn main() {
    let mut logger_builder = env_logger::Builder::from_default_env();
    logger_builder.target(env_logger::Target::Stdout);
    logger_builder.filter_level(log::LevelFilter::Info);
    logger_builder.init();

    let args: Vec<String> = std::env::args().collect();

    // cosdup <src> <out>
    if args.len() == 3 {
        // 指定压缩目录和下载最大目录数量，太大占有磁盘空间
        let mut dup = Dup::new(PathBuf::from(args.get(2).unwrap().as_str()), 40);
        // 指定下载目录
        dup.start_download(args.get(1).unwrap().as_str());
    }else {
        println!("cosdup <src> <target>");
    }
}