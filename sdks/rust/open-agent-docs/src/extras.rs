//! 生成的接口之外，手写的几个顺手的写法。
use crate::error::Result;
use crate::resources::PagesApi;
use crate::types::{CreatePage, Page, PutContent};

impl PagesApi {
    /// 建页面，再把 Markdown 写成正文（`POST /workspaces/{ws}/pages` 之后 `PUT /pages/{id}/content`）。
    ///
    /// ```no_run
    /// # use open_agent_docs as oad;
    /// # async fn run(client: oad::Client, ws: &str) -> oad::Result<()> {
    /// use oad::types::CreatePage;
    /// let page = client
    ///     .pages()
    ///     .create_with_markdown(ws, CreatePage { title: "周报".into(), ..Default::default() }, "# 标题\n\n正文")
    ///     .await?;
    /// # let _ = page;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_with_markdown(
        &self,
        ws: impl Into<String>,
        body: CreatePage,
        markdown: impl Into<String>,
    ) -> Result<Page> {
        let page = self.create(ws, body).await?;
        let content = PutContent {
            markdown: markdown.into().into(),
            ..Default::default()
        };
        self.content().replace(&page.id, content).await?;
        Ok(page)
    }
}
