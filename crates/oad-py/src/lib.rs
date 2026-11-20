//! Python 绑定（PyPI 上的 open-agent-docs-server，模块 open_agent_docs_server）：在 Python 进程里起
//! Open Agent Docs 服务器（自己的线程和 tokio 运行时），配置项和命令行程序一样（`OAD_*`，也可以写成
//! `data_dir=` 这样的关键字参数）。一般不直接用：装 `open-agent-docs[server]`，SDK 的 `oad.start()` 会用它。
//!
//! ```python
//! import open_agent_docs as oad          # SDK
//! with oad.start(data_dir="./data", auth="none") as server:
//!     print(server.url)
//! ```
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Mutex;

/// 跑着的服务器。可以当上下文管理器用（退出时关掉）。
#[pyclass(module = "open_agent_docs_server")]
struct Server {
    #[pyo3(get)]
    url: String,
    #[pyo3(get)]
    port: u16,
    inner: Mutex<Option<oad_bin::Background>>,
}

#[pymethods]
impl Server {
    /// 关掉（页面都存好后返回）；关过了再调什么也不做。
    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let b = self.inner.lock().unwrap().take();
        if let Some(mut b) = b {
            py.detach(move || b.close())
                .map_err(PyRuntimeError::new_err)?;
        }
        Ok(())
    }

    #[getter]
    fn closed(&self) -> bool {
        self.inner.lock().unwrap().is_none()
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    #[pyo3(signature = (*_args))]
    fn __exit__(&self, py: Python<'_>, _args: &Bound<'_, pyo3::types::PyTuple>) -> PyResult<bool> {
        self.close(py)?;
        Ok(false)
    }

    fn __repr__(&self) -> String {
        format!("<open_agent_docs_server.Server {}>", self.url)
    }
}

/// 起服务器，监听好了返回。关键字参数是配置项（`OAD_*` 或 `data_dir`、`auth`、`port`……），
/// 值转成字符串（列表用逗号连起来，True / False 是 1 / 0）；不给 port 时随便挑一个空端口。
#[pyfunction]
#[pyo3(signature = (**options))]
fn start(py: Python<'_>, options: Option<&Bound<'_, PyDict>>) -> PyResult<Server> {
    let mut vars: Vec<(String, String)> = vec![("OAD_PORT".into(), "0".into())];
    if let Some(d) = options {
        for (k, v) in d.iter() {
            if v.is_none() {
                continue;
            }
            let k: String = k.extract()?;
            let v = if let Ok(b) = v.extract::<bool>() {
                if b { "1".to_string() } else { "0".to_string() }
            } else if let Ok(list) = v.extract::<Vec<String>>() {
                list.join(",")
            } else {
                v.str()?.to_string()
            };
            vars.push((k, v));
        }
    }
    oad_bin::init_logging();
    let env = oad_bin::Env::with(vars);
    let b = py
        .detach(move || oad_bin::Background::start(env))
        .map_err(PyRuntimeError::new_err)?;
    Ok(Server {
        url: b.url.clone(),
        port: b.port,
        inner: Mutex::new(Some(b)),
    })
}

#[pymodule]
fn open_agent_docs_server(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Server>()?;
    m.add_function(wrap_pyfunction!(start, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
