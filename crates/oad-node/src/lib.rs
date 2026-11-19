//! Node 绑定（@oad/server）：在 Node 进程里起 Open Agent Docs 服务器（自己的线程和 tokio 运行时），
//! 配置项和命令行程序一样（`OAD_*`，可以写成 `dataDir` 这样）。JS 那边的包装在 packages/server-node/index.js。
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 跑着的服务器。
#[napi]
pub struct Server {
    url: String,
    port: u32,
    inner: Arc<Mutex<Option<oad_bin::Background>>>,
}

#[napi]
impl Server {
    /// `http://127.0.0.1:<端口>`
    #[napi(getter)]
    pub fn url(&self) -> String {
        self.url.clone()
    }

    #[napi(getter)]
    pub fn port(&self) -> u32 {
        self.port
    }

    /// 关掉（页面都存好后 resolve）；关过了再调直接 resolve。
    #[napi]
    pub fn close(&self) -> AsyncTask<Close> {
        AsyncTask::new(Close(self.inner.clone()))
    }
}

pub struct Close(Arc<Mutex<Option<oad_bin::Background>>>);

impl Task for Close {
    type Output = ();
    type JsValue = ();
    fn compute(&mut self) -> Result<()> {
        let b = self.0.lock().unwrap().take();
        if let Some(mut b) = b {
            b.close().map_err(Error::from_reason)?;
        }
        Ok(())
    }
    fn resolve(&mut self, _: Env, _: ()) -> Result<()> {
        Ok(())
    }
}

pub struct Start(Option<HashMap<String, String>>);

impl Task for Start {
    type Output = oad_bin::Background;
    type JsValue = Server;
    fn compute(&mut self) -> Result<oad_bin::Background> {
        oad_bin::init_logging();
        let env = oad_bin::Env::with(self.0.take().unwrap_or_default());
        oad_bin::Background::start(env).map_err(Error::from_reason)
    }
    fn resolve(&mut self, _: Env, b: oad_bin::Background) -> Result<Server> {
        Ok(Server {
            url: b.url.clone(),
            port: b.port as u32,
            inner: Arc::new(Mutex::new(Some(b))),
        })
    }
}

/// 起服务器，监听好了 resolve。options 的值都是字符串（JS 包装会转）。
#[napi]
pub fn start(options: Option<HashMap<String, String>>) -> AsyncTask<Start> {
    AsyncTask::new(Start(options))
}
