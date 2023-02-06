use cookie_store::CookieStore;
use reqwest::redirect::Policy;
use reqwest_cookie_store::CookieStoreMutex;
use log::{error, warn, info};
use reqwest::Client;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct State
{
    cookie_store_path: PathBuf,
    cookie_store: Arc<CookieStoreMutex>,
    load_session: bool
}

impl State {
    pub fn try_new(cookie_store_path: PathBuf) -> anyhow::Result<State> {
        let mut have = false;
        let cookie_store = match File::open(&cookie_store_path).map(std::io::BufReader::new) {
            Ok(f) => {
                match CookieStore::load_json(f) {
                    Ok(v) => {
                        have = true;
                        v
                    },
                    Err(e) => {
                        error!("load cookie json error: {}", e);
                        CookieStore::default()
                    }
                }
            },
            Err(e) => {
                warn!(
                    "open {} failed. error: {}, use default empty cookie store",
                    cookie_store_path.display(),
                    e
                );
                CookieStore::default()
            }
        };
        let cookie_store = Arc::new(CookieStoreMutex::new(cookie_store));

        Ok(State {
            cookie_store_path,
            cookie_store,
            load_session: have
        })
    }

    #[allow(dead_code)]
    pub fn have_session(self: &Self) -> bool {
        return self.load_session;
    }
}

impl Drop for State {
    fn drop(&mut self) {
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.cookie_store_path)
            .map(std::io::BufWriter::new)
        {
            Ok(f) => f,
            Err(e) => {
                error!(
                    "open {} for write failed. error: {}",
                    self.cookie_store_path.display(),
                    e
                );
                return;
            }
        };

        let store = self.cookie_store.lock().unwrap();
        if let Err(e) = store.save_incl_expired_and_nonpersistent_json(&mut file) {
            error!(
                "save cookies to path {} failed. error: {}",
                self.cookie_store_path.display(),
                e
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct Session
{
    #[allow(dead_code)]
    state: Arc<State>,
    client: Client
}

impl Session {
    pub fn try_new(cookie_store_path: PathBuf) -> anyhow::Result<Session> {
        let state = State::try_new(cookie_store_path)?;
        let state = Arc::new(state);

        let client = Client::builder()
            .cookie_provider(Arc::clone(&state.cookie_store))
            .redirect(Policy::limited(5))
            .build()?;

        Ok(Session { state, client })
    }

    #[allow(dead_code)]
    pub fn get_ref(self: &Self) -> &Client {
        &self.client
    }
    #[allow(dead_code)]
    pub fn get_mut_ref(self: &mut Self) -> &mut Client {
        &mut self.client
    }

    #[allow(dead_code)]
    pub fn get_cookie_store(self: &Self) -> &CookieStoreMutex {
        return self.state.cookie_store.as_ref();
    }

    pub async fn http_get(self: &Self, url: &str) -> Result<reqwest::Response, reqwest::Error> {
        return self.client.get(url)
            .header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/109.0.0.0 Safari/537.36")
            .header("Referer", "https://www.cosjun.cn/")
            .header("Sec-Ch-Ua-Platform", "Linux")
            .header("Accept_Language", "zh-CN,zh;q=0.9")
            .send().await;
    }

    #[allow(dead_code)]
    pub async fn login(self: &mut Self, username: &str, password: &str) -> bool {
        if self.state.have_session() {
            warn!("session is load, login skip!");
            return true;
        }
        let mut un = String::new();
        url_escape::encode_path_to_string(username, &mut un);
        let mut pa = String::new();
        url_escape::encode_path_to_string(password, &mut pa);
        let data = format!("action=user_login&username={}&password={}", username, password);
        info!("login data: {}", &data);
        let res = self.client.post("https://www.cosjun.cn/wp-admin/admin-ajax.php")
                .version(reqwest::Version::HTTP_11)
                .body(data)
                .header("Content-Type", "application/x-www-form-urlencoded; charset=UTF-8")
                .header("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/109.0.0.0 Safari/537.36")
                .header("Referer", "https://www.cosjun.cn/")
                .header("Sec-Ch-Ua-Platform", "Linux")
                .header("Accept_Language", "zh-CN,zh;q=0.9")
                .header("Accept", "*/*")
                .send().await;
        match res {
            Ok(res) => {
                match res.text().await {
                    Ok(res) => {
                        if res.contains("\"status\":\"1\"") {
                            info!("cosjun ==> login success!");
                            return true;
                        }
                        error!("login msg: {}", &res);
                    },
                    Err(e) => {
                        error!("cosjun ==> login post request text error: {}", e.to_string());
                    }
                }
            },
            Err(e) => {
                error!("cosjun ==> login post request error: {}", e.to_string());
            }
        }
        return false;
    }

    #[allow(dead_code)]
    pub async fn logout(self: &mut Self) {
        let _ = self.http_get(
            "https://www.cosjun.cn/wp-login.php?action=logout&redirect_to=https%3A%2F%2Fwww.cosjun.cn"
            ).await;
    }
}


