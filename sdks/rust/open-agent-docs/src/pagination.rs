//! 翻页（契约 §6）：带 `next_cursor` 的列表。`list(...)` 是一页，`list_all(...)` 逐条取完所有页。
use crate::error::Result;
use futures_util::future::BoxFuture;
use futures_util::stream::{BoxStream, Stream, StreamExt};
use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

/// 取一页的结果：（这页的条目，下一页的游标）
pub type PageFuture<T> = BoxFuture<'static, Result<(Vec<T>, Option<String>)>>;

/// 能按游标翻页的请求（生成的 `…ListRequest` 实现它）。
pub trait Paginated: Clone + Send + Sync + Unpin + 'static {
    /// 一条
    type Item: Send + 'static;

    /// 请求上已经设的游标（从哪一页开始）
    fn cursor(&self) -> Option<String>;

    /// 换一个游标的同一个请求
    fn with_cursor(&self, cursor: Option<String>) -> Self;

    /// 取一页：（这页的条目，下一页的游标）
    fn fetch(self) -> PageFuture<Self::Item>;
}

/// `list_all(...)` 返回的东西：一个 `Stream<Item = Result<T>>`，逐条给出所有页的条目。
///
/// 第一次取的时候才发请求；在那之前可以像 `list(...)` 一样设参数（`.limit(50)`）。
/// 停止条件和 JS 的 `paginate` 一样：`next_cursor` 是空的、这页没有条目、或者游标没变。出错时给出错误然后结束。
///
/// ```no_run
/// # use open_agent_docs as oad;
/// use futures_util::TryStreamExt;
/// # async fn run(client: oad::Client, page_id: &str) -> oad::Result<()> {
/// let mut versions = client.pages().versions().list_all(page_id).limit(50);
/// while let Some(v) = versions.try_next().await? {
///     println!("{} {:?}", v.id, v.label);
/// }
/// # Ok(())
/// # }
/// ```
#[must_use = "Stream 要取（.next() / .try_collect()）才会发请求"]
pub struct ListAll<R: Paginated> {
    req: Option<R>,
    stream: Option<BoxStream<'static, Result<R::Item>>>,
}

impl<R: Paginated> ListAll<R> {
    pub(crate) fn new(req: R) -> Self {
        ListAll {
            req: Some(req),
            stream: None,
        }
    }

    /// 改一下请求（已经开始取了就没有用了）
    pub(crate) fn map_request(mut self, f: impl FnOnce(R) -> R) -> Self {
        self.req = self.req.take().map(f);
        self
    }
}

impl<R: Paginated + std::fmt::Debug> std::fmt::Debug for ListAll<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListAll")
            .field("req", &self.req)
            .field("started", &self.stream.is_some())
            .finish()
    }
}

struct State<R: Paginated> {
    req: R,
    cursor: Option<String>,
    buf: VecDeque<R::Item>,
    done: bool,
}

fn paginate<R: Paginated>(req: R) -> BoxStream<'static, Result<R::Item>> {
    let state = State {
        cursor: req.cursor(),
        req,
        buf: VecDeque::new(),
        done: false,
    };
    futures_util::stream::unfold(state, |mut s| async move {
        loop {
            if let Some(x) = s.buf.pop_front() {
                return Some((Ok(x), s));
            }
            if s.done {
                return None;
            }
            match s.req.with_cursor(s.cursor.clone()).fetch().await {
                Err(e) => {
                    s.done = true;
                    return Some((Err(e), s));
                }
                Ok((items, next)) => {
                    let next = next.filter(|c| !c.is_empty());
                    // 没有下一页、这页是空的、或者游标没往前走（服务器的毛病）：取完这页就停
                    if next.is_none() || items.is_empty() || next == s.cursor {
                        s.done = true;
                    }
                    s.cursor = next;
                    s.buf.extend(items);
                }
            }
        }
    })
    .boxed()
}

impl<R: Paginated> Stream for ListAll<R> {
    type Item = Result<R::Item>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.stream.is_none() {
            match this.req.take() {
                Some(r) => this.stream = Some(paginate(r)),
                None => return Poll::Ready(None),
            }
        }
        this.stream
            .as_mut()
            .expect("上面刚放进去")
            .poll_next_unpin(cx)
    }
}
