use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Conflict(String),
    /// 去别的网站取东西失败了（链接解析）。
    #[error("{0}")]
    BadGateway(String),
    #[error("存储出错：{0}")]
    Storage(String),
    #[error("内部错误：{0}")]
    Internal(String),
}

impl Error {
    /// 接口里的错误码。
    pub fn code(&self) -> &'static str {
        match self {
            Error::NotFound(_) => "not_found",
            Error::BadRequest(_) => "bad_request",
            Error::Unauthorized(_) => "unauthorized",
            Error::Forbidden(_) => "forbidden",
            Error::Conflict(_) => "conflict",
            Error::BadGateway(_) => "bad_gateway",
            Error::Storage(_) | Error::Internal(_) => "internal",
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Storage(e.to_string())
    }
}

impl From<oad_doc::Error> for Error {
    fn from(e: oad_doc::Error) -> Self {
        match e {
            oad_doc::Error::NoBlock(id) => Error::NotFound(format!("找不到块 {id}")),
            e => Error::BadRequest(e.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
