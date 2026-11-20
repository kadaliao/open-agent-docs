# 由 codegen 生成，不要手改：python sdks/python/codegen/generate.py（来源 docs/openapi.json、sdks/operations.json）
# pyright: reportPrivateUsage=false
"""资源类：client.pages.versions.list(...) 这样的方法（名字见 sdks/operations.json）。"""
from __future__ import annotations

from functools import cached_property
from typing import Any, Dict, List, Mapping, Optional, Sequence, Union

from .._base_client import AsyncAPIClient, Body, Headers, Query, SyncAPIClient, TimeoutTypes
from .._files import FileTypes, csv_body as _csv_body, file_part as _file_part
from .._pagination import AsyncPaginator, CursorPage
from .._sse import AsyncEventStream, EventStream
from .._types import NOT_GIVEN, NotGiven
from .._utils import quote_path as _p
from .models import (
    Acked,
    AddRow,
    AddedRows,
    Agent,
    AgentNotes,
    AgentNotification,
    AgentView,
    Anchor,
    AuthConfigBody,
    AuthorRef,
    BlockContent,
    BlockIds,
    CellOp,
    CommentView,
    Content,
    CreatedAgent,
    DbHistory,
    DbHistoryItem,
    DbInfo,
    DbUndone,
    DbView,
    Deleted,
    DocNode,
    Inbox,
    InboxNotification,
    InviteInfo,
    InviteView,
    Me,
    MemberList,
    MemberRole,
    NewSecret,
    NewToken,
    Notify,
    Page,
    Patched,
    Pressed,
    PutViews,
    QueryResult,
    ReadCount,
    Restored,
    RestoredPage,
    RowsDelta,
    SchemaChanged,
    SchemaUndone,
    SearchResult,
    Seq,
    ShareView,
    ThreadView,
    TrashItem,
    TrashList,
    UnfurlCard,
    Uploaded,
    VersionContent,
    VersionDiff,
    VersionList,
    VersionMeta,
    Workspace,
)

__all__ = ["SyncResources", "AsyncResources"]


class AgentNotificationsResource:
    """`client.agent.notifications`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        *,
        after: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        wait: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CursorPage[AgentNotification]:
        """Agent 取通知（长轮询）：没有新的就等 wait 秒。不给 after / cursor 就是还没处理（ack）的。只有 Agent 能调。

        `GET /api/v1/agent/notifications`
        """
        return self._client._call_page(
            "GET",
            "/agent/notifications",
            query={"after": after, "cursor": cursor, "limit": limit, "wait": wait},
            model=AgentNotes,
            items="notifications",
            item=AgentNotification,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def ack(
        self,
        *,
        ids: Union[Sequence[int], NotGiven] = NOT_GIVEN,
        upto: Union[int, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Acked:
        """`POST /api/v1/agent/notifications/ack`"""
        return self._client._call(
            "POST",
            "/agent/notifications/ack",
            body={"ids": ids, "upto": upto},
            extra_body=extra_body,
            cast=Acked,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def stream(
        self,
        *,
        after: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> EventStream:
        """SSE：先补发游标之后的（`?after=` / `?cursor=` 或 `Last-Event-ID` 头；都没有就是没处理的），再实时推。每条是 `event: notification`，`id` 是通知编号，`data` 是 AgentNotification。每 25 秒一个注释行保活。只有 Agent 能调。

        `GET /api/v1/agent/notifications/stream`
        """
        return self._client._stream(
            "/agent/notifications/stream",
            query={"after": after, "cursor": cursor},
            kind="agent",
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AgentResource:
    """`client.agent`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    @cached_property
    def notifications(self) -> AgentNotificationsResource:
        return AgentNotificationsResource(self._client)


class AgentsResource:
    """`client.agents`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        ws: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[AgentView]:
        """`GET /api/v1/workspaces/{ws}/agents`"""
        return self._client._call(
            "GET",
            f"/workspaces/{_p(ws)}/agents",
            unwrap="agents",
            cast=List[AgentView],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def create(
        self,
        ws: str,
        *,
        name: str,
        scope: str,
        about: Union[str, None, NotGiven] = NOT_GIVEN,
        webhook: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CreatedAgent:
        """`POST /api/v1/workspaces/{ws}/agents`"""
        return self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/agents",
            body={"about": about, "name": name, "scope": scope, "webhook": webhook},
            extra_body=extra_body,
            cast=CreatedAgent,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def update(
        self,
        id: str,
        *,
        about: Union[str, None, NotGiven] = NOT_GIVEN,
        locale: Union[str, None, NotGiven] = NOT_GIVEN,
        name: Union[str, None, NotGiven] = NOT_GIVEN,
        scope: Union[str, None, NotGiven] = NOT_GIVEN,
        webhook: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Agent:
        """`PATCH /api/v1/agents/{id}`"""
        return self._client._call(
            "PATCH",
            f"/agents/{_p(id)}",
            body={"about": about, "locale": locale, "name": name, "scope": scope, "webhook": webhook},
            extra_body=extra_body,
            cast=Agent,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def revoke(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Agent:
        """撤销：token 马上失效，返回撤销后的 Agent（带 revoked_at）。

        `DELETE /api/v1/agents/{id}`
        """
        return self._client._call(
            "DELETE",
            f"/agents/{_p(id)}",
            cast=Agent,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def rotate_token(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> NewToken:
        """`POST /api/v1/agents/{id}/token`"""
        return self._client._call(
            "POST",
            f"/agents/{_p(id)}/token",
            cast=NewToken,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def rotate_webhook_secret(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> NewSecret:
        """`POST /api/v1/agents/{id}/webhook-secret`"""
        return self._client._call(
            "POST",
            f"/agents/{_p(id)}/webhook-secret",
            cast=NewSecret,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AuthResource:
    """`client.auth`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def config(
        self,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AuthConfigBody:
        """`GET /api/v1/auth/config`"""
        return self._client._call(
            "GET",
            "/auth/config",
            cast=AuthConfigBody,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def login(
        self,
        *,
        email: str,
        password: str,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Me:
        """`POST /api/v1/auth/login`"""
        return self._client._call(
            "POST",
            "/auth/login",
            body={"email": email, "password": password},
            extra_body=extra_body,
            cast=Me,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def logout(
        self,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`POST /api/v1/auth/logout`"""
        return self._client._call(
            "POST",
            "/auth/logout",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def signup(
        self,
        *,
        email: str,
        password: str,
        invite: Union[str, None, NotGiven] = NOT_GIVEN,
        name: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Me:
        """注册（本地登录模式）。OAD_SIGNUP：invite（默认，要凭邀请）、open、off。

        `POST /api/v1/auth/signup`
        """
        return self._client._call(
            "POST",
            "/auth/signup",
            body={"email": email, "invite": invite, "name": name, "password": password},
            extra_body=extra_body,
            cast=Me,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class CommentsResource:
    """`client.comments`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def update(
        self,
        id: str,
        *,
        body: str,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CommentView:
        """只有作者能改。

        `PATCH /api/v1/comments/{id}`
        """
        return self._client._call(
            "PATCH",
            f"/comments/{_p(id)}",
            body={"body": body},
            extra_body=extra_body,
            cast=CommentView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def delete(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`DELETE /api/v1/comments/{id}`"""
        return self._client._call(
            "DELETE",
            f"/comments/{_p(id)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class DatabasesHistoryResource:
    """`client.databases.history`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        id: str,
        *,
        row: Union[str, NotGiven] = NOT_GIVEN,
        prop: Union[str, NotGiven] = NOT_GIVEN,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CursorPage[DbHistoryItem]:
        """`GET /api/v1/databases/{id}/history`"""
        return self._client._call_page(
            "GET",
            f"/databases/{_p(id)}/history",
            query={"row": row, "prop": prop, "limit": limit, "cursor": cursor},
            model=DbHistory,
            items="items",
            item=DbHistoryItem,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def undo(
        self,
        id: str,
        *,
        seq: int,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbUndone:
        """`POST /api/v1/databases/{id}/history/undo`"""
        return self._client._call(
            "POST",
            f"/databases/{_p(id)}/history/undo",
            body={"seq": seq},
            extra_body=extra_body,
            cast=DbUndone,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class DatabasesRowsResource:
    """`client.databases.rows`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        id: str,
        *,
        since: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> RowsDelta:
        """行（全量或增量）。按 Accept-Encoding 压缩：br 优先，其次 gzip（1 万行的全量压缩后 < 1MB）。

        `GET /api/v1/databases/{id}/rows`
        """
        return self._client._call(
            "GET",
            f"/databases/{_p(id)}/rows",
            query={"since": since},
            cast=RowsDelta,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def add(
        self,
        id: str,
        *,
        rows: Sequence[Union[AddRow, Dict[str, Any]]],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AddedRows:
        """`POST /api/v1/databases/{id}/rows`"""
        return self._client._call(
            "POST",
            f"/databases/{_p(id)}/rows",
            body={"rows": rows},
            extra_body=extra_body,
            cast=AddedRows,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def update(
        self,
        id: str,
        *,
        ops: Sequence[Union[CellOp, Dict[str, Any]]],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Patched:
        """`PATCH /api/v1/databases/{id}/rows`"""
        return self._client._call(
            "PATCH",
            f"/databases/{_p(id)}/rows",
            body={"ops": ops},
            extra_body=extra_body,
            cast=Patched,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def delete(
        self,
        id: str,
        *,
        ids: Sequence[str],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Seq:
        """`DELETE /api/v1/databases/{id}/rows`"""
        return self._client._call(
            "DELETE",
            f"/databases/{_p(id)}/rows",
            body={"ids": ids},
            extra_body=extra_body,
            cast=Seq,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def delete_one(
        self,
        id: str,
        row: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Seq:
        """`DELETE /api/v1/databases/{id}/rows/{row}`"""
        return self._client._call(
            "DELETE",
            f"/databases/{_p(id)}/rows/{_p(row)}",
            cast=Seq,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class DatabasesSchemaResource:
    """`client.databases.schema`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def update(
        self,
        id: str,
        *,
        ops: Sequence[Mapping[str, Any]],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> SchemaChanged:
        """`PATCH /api/v1/databases/{id}/schema`"""
        return self._client._call(
            "PATCH",
            f"/databases/{_p(id)}/schema",
            body={"ops": ops},
            extra_body=extra_body,
            cast=SchemaChanged,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def undo(
        self,
        id: str,
        *,
        seq: int,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> SchemaUndone:
        """`POST /api/v1/databases/{id}/schema/undo`"""
        return self._client._call(
            "POST",
            f"/databases/{_p(id)}/schema/undo",
            body={"seq": seq},
            extra_body=extra_body,
            cast=SchemaUndone,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class DatabasesViewsResource:
    """`client.databases.views`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def create(
        self,
        id: str,
        *,
        body: Mapping[str, Any],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbView:
        """加一个视图（不给 type 是表格），返回它（带上新的 id）。

        `POST /api/v1/databases/{id}/views`
        """
        return self._client._call(
            "POST",
            f"/databases/{_p(id)}/views",
            body=body,
            cast=DbView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def replace(
        self,
        id: str,
        *,
        body: PutViews,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[DbView]:
        """整个换掉：`[...]` 或 `{ views: [...] }`。

        `PUT /api/v1/databases/{id}/views`
        """
        return self._client._call(
            "PUT",
            f"/databases/{_p(id)}/views",
            body=body,
            unwrap="views",
            cast=List[DbView],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def update(
        self,
        id: str,
        view: str,
        *,
        body: Mapping[str, Any],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbView:
        """改一个视图的几项：给了的字段覆盖，null 去掉。返回改好的视图。

        `PATCH /api/v1/databases/{id}/views/{view}`
        """
        return self._client._call(
            "PATCH",
            f"/databases/{_p(id)}/views/{_p(view)}",
            body=body,
            cast=DbView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def delete(
        self,
        id: str,
        view: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """删一个视图（最后一个不能删）。

        `DELETE /api/v1/databases/{id}/views/{view}`
        """
        return self._client._call(
            "DELETE",
            f"/databases/{_p(id)}/views/{_p(view)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class DatabasesResource:
    """`client.databases`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    @cached_property
    def history(self) -> DatabasesHistoryResource:
        return DatabasesHistoryResource(self._client)

    @cached_property
    def rows(self) -> DatabasesRowsResource:
        return DatabasesRowsResource(self._client)

    @cached_property
    def schema(self) -> DatabasesSchemaResource:
        return DatabasesSchemaResource(self._client)

    @cached_property
    def views(self) -> DatabasesViewsResource:
        return DatabasesViewsResource(self._client)

    def create(
        self,
        ws: str,
        *,
        after: Union[str, None, NotGiven] = NOT_GIVEN,
        parent_id: Union[str, None, NotGiven] = NOT_GIVEN,
        schema: Union[Mapping[str, Any], None, NotGiven] = NOT_GIVEN,
        title: Union[str, None, NotGiven] = NOT_GIVEN,
        views: Union[Sequence[Mapping[str, Any]], None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbInfo:
        """`POST /api/v1/workspaces/{ws}/databases`"""
        return self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/databases",
            body={"after": after, "parent_id": parent_id, "schema": schema, "title": title, "views": views},
            extra_body=extra_body,
            cast=DbInfo,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def import_csv(
        self,
        ws: str,
        csv: Union[str, bytes],
        *,
        title: Union[str, NotGiven] = NOT_GIVEN,
        parent_id: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbInfo:
        """导入 CSV：multipart 的 `file`（和可选的 `title`、`parent_id`），或者原始正文（`?title=`）。

        `POST /api/v1/workspaces/{ws}/databases/import`
        """
        return self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/databases/import",
            query={"title": title, "parent_id": parent_id},
            content=_csv_body(csv),
            headers={"content-type": "text/csv; charset=utf-8"},
            cast=DbInfo,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def get(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbInfo:
        """库的页面、表结构、视图、现在的 seq。

        `GET /api/v1/databases/{id}`
        """
        return self._client._call(
            "GET",
            f"/databases/{_p(id)}",
            cast=DbInfo,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def query(
        self,
        id: str,
        *,
        filter: Union[Mapping[str, Any], None, NotGiven] = NOT_GIVEN,
        group_by: Union[str, None, NotGiven] = NOT_GIVEN,
        limit: Union[int, None, NotGiven] = NOT_GIVEN,
        offset: Union[int, None, NotGiven] = NOT_GIVEN,
        search: Union[str, None, NotGiven] = NOT_GIVEN,
        sorts: Union[Sequence[Mapping[str, Any]], None, NotGiven] = NOT_GIVEN,
        totals: Union[Mapping[str, Any], None, NotGiven] = NOT_GIVEN,
        view_id: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> QueryResult:
        """服务器算的查询：给了 view_id 就从那个视图出发，别的字段覆盖它。

        `POST /api/v1/databases/{id}/query`
        """
        return self._client._call(
            "POST",
            f"/databases/{_p(id)}/query",
            body={"filter": filter, "group_by": group_by, "limit": limit, "offset": offset, "search": search, "sorts": sorts, "totals": totals, "view_id": view_id},
            extra_body=extra_body,
            cast=QueryResult,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def export_csv(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> bytes:
        """导出 CSV（UTF-8，按第一个视图的列）。

        `GET /api/v1/databases/{id}/export.csv`
        """
        return self._client._call(
            "GET",
            f"/databases/{_p(id)}/export.csv",
            cast=bytes,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def press_button(
        self,
        id: str,
        row: str,
        key: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Pressed:
        """按一行上的按钮：按设置改属性，有 `comment` 的在这一行上发一条评论（可以叫 Agent）。

        `POST /api/v1/databases/{id}/button/{row}/{key}`
        """
        return self._client._call(
            "POST",
            f"/databases/{_p(id)}/button/{_p(row)}/{_p(key)}",
            cast=Pressed,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class FilesResource:
    """`client.files`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def upload(
        self,
        ws: str,
        file: FileTypes,
        filename: Optional[str] = None,
        content_type: Optional[str] = None,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Uploaded:
        """上传文件：`multipart/form-data` 的 `file` 字段，或者原始正文 + `X-Filename` 头（文件名按网址规则编码）。大小上限见 OAD_MAX_UPLOAD。图片会算宽高和 blurhash，音视频算时长。

        `POST /api/v1/workspaces/{ws}/files`
        """
        return self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/files",
            files=_file_part(file, filename, content_type),
            cast=Uploaded,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def download(
        self,
        id: str,
        *,
        w: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> bytes:
        """同 `/files/{id}/{name}`，不带文件名。

        `GET /api/v1/files/{id}`
        """
        return self._client._call(
            "GET",
            f"/files/{_p(id)}",
            query={"w": w},
            cast=bytes,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def download_named(
        self,
        id: str,
        name: str,
        *,
        w: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> bytes:
        """下载上传的文件：原来的类型；带 ETag、长缓存、Range。`?w=` 给图片的缩略图（WebP）。SVG、HTML 之类带 `Content-Security-Policy: sandbox`，SVG 当附件下载。不是这个工作区的成员当不存在（404）。

        `GET /api/v1/files/{id}/{name}`
        """
        return self._client._call(
            "GET",
            f"/files/{_p(id)}/{_p(name)}",
            query={"w": w},
            cast=bytes,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def media(
        self,
        hash: str,
        *,
        w: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> bytes:
        """转存的外部图片（链接卡片的封面、头像之类），按内容的 hash。登录了就能看。`?w=` 给缩略图（WebP），支持 Range。

        `GET /api/v1/media/{hash}`
        """
        return self._client._call(
            "GET",
            f"/media/{_p(hash)}",
            query={"w": w},
            cast=bytes,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class InvitesResource:
    """`client.invites`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def get(
        self,
        token: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> InviteInfo:
        """打开邀请链接时看的：哪个工作区、什么角色、谁邀请的。不用登录。

        `GET /api/v1/invites/{token}`
        """
        return self._client._call(
            "GET",
            f"/invites/{_p(token)}",
            cast=InviteInfo,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def accept(
        self,
        token: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Me:
        """接受邀请，加入工作区（已经在里面的不降角色）。返回加入后的 /me。

        `POST /api/v1/invites/{token}/accept`
        """
        return self._client._call(
            "POST",
            f"/invites/{_p(token)}/accept",
            cast=Me,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def list(
        self,
        ws: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[InviteView]:
        """`GET /api/v1/workspaces/{ws}/invites`"""
        return self._client._call(
            "GET",
            f"/workspaces/{_p(ws)}/invites",
            unwrap="invites",
            cast=List[InviteView],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def create(
        self,
        ws: str,
        *,
        role: str,
        expires_in: Union[int, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> InviteView:
        """`POST /api/v1/workspaces/{ws}/invites`"""
        return self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/invites",
            body={"expires_in": expires_in, "role": role},
            extra_body=extra_body,
            cast=InviteView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def revoke(
        self,
        ws: str,
        token: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`DELETE /api/v1/workspaces/{ws}/invites/{token}`"""
        return self._client._call(
            "DELETE",
            f"/workspaces/{_p(ws)}/invites/{_p(token)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class LinksResource:
    """`client.links`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def unfurl(
        self,
        *,
        url: str,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> UnfurlCard:
        """解析一个网址成卡片：上传的文件直接从库里给，本站的库给视图列表，别的网址去抓（有缓存，图片转存到 /media）。

        `GET /api/v1/unfurl`
        """
        return self._client._call(
            "GET",
            "/unfurl",
            query={"url": url},
            cast=UnfurlCard,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class MeNotificationsResource:
    """`client.me.notifications`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        *,
        after: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CursorPage[InboxNotification]:
        """`GET /api/v1/me/notifications`"""
        return self._client._call_page(
            "GET",
            "/me/notifications",
            query={"after": after, "cursor": cursor, "limit": limit},
            model=Inbox,
            items="notifications",
            item=InboxNotification,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def mark_read(
        self,
        *,
        all: Union[bool, NotGiven] = NOT_GIVEN,
        ids: Union[Sequence[int], None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> ReadCount:
        """`POST /api/v1/me/notifications/read`"""
        return self._client._call(
            "POST",
            "/me/notifications/read",
            body={"all": all, "ids": ids},
            extra_body=extra_body,
            cast=ReadCount,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class MeResource:
    """`client.me`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    @cached_property
    def notifications(self) -> MeNotificationsResource:
        return MeNotificationsResource(self._client)

    def get(
        self,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Me:
        """`GET /api/v1/me`"""
        return self._client._call(
            "GET",
            "/me",
            cast=Me,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def update(
        self,
        *,
        avatar: Union[str, None, NotGiven] = NOT_GIVEN,
        locale: Union[str, None, NotGiven] = NOT_GIVEN,
        name: Union[str, None, NotGiven] = NOT_GIVEN,
        timezone: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Me:
        """`PATCH /api/v1/me`"""
        return self._client._call(
            "PATCH",
            "/me",
            body={"avatar": avatar, "locale": locale, "name": name, "timezone": timezone},
            extra_body=extra_body,
            cast=Me,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def change_password(
        self,
        *,
        current: str,
        new: str,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """改密码（本地登录）。别的会话都退出，这次请求用的会话保留。

        `POST /api/v1/me/password`
        """
        return self._client._call(
            "POST",
            "/me/password",
            body={"current": current, "new": new},
            extra_body=extra_body,
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class PagesBlocksResource:
    """`client.pages.blocks`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def insert(
        self,
        id: str,
        *,
        markdown: str,
        after: Union[str, None, NotGiven] = NOT_GIVEN,
        base: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> BlockIds:
        """`POST /api/v1/pages/{id}/blocks`"""
        return self._client._call(
            "POST",
            f"/pages/{_p(id)}/blocks",
            body={"after": after, "base": base, "markdown": markdown},
            extra_body=extra_body,
            cast=BlockIds,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def get(
        self,
        id: str,
        block_id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> BlockContent:
        """`GET /api/v1/pages/{id}/blocks/{block_id}`"""
        return self._client._call(
            "GET",
            f"/pages/{_p(id)}/blocks/{_p(block_id)}",
            cast=BlockContent,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def replace(
        self,
        id: str,
        block_id: str,
        *,
        markdown: str,
        after: Union[str, None, NotGiven] = NOT_GIVEN,
        base: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> BlockIds:
        """`PUT /api/v1/pages/{id}/blocks/{block_id}`"""
        return self._client._call(
            "PUT",
            f"/pages/{_p(id)}/blocks/{_p(block_id)}",
            body={"after": after, "base": base, "markdown": markdown},
            extra_body=extra_body,
            cast=BlockIds,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def delete(
        self,
        id: str,
        block_id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`DELETE /api/v1/pages/{id}/blocks/{block_id}`"""
        return self._client._call(
            "DELETE",
            f"/pages/{_p(id)}/blocks/{_p(block_id)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class PagesContentResource:
    """`client.pages.content`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def get(
        self,
        id: str,
        *,
        format: Union[str, NotGiven] = NOT_GIVEN,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Content:
        """`GET /api/v1/pages/{id}/content`"""
        return self._client._call(
            "GET",
            f"/pages/{_p(id)}/content",
            query={"format": format, "limit": limit},
            cast=Content,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def replace(
        self,
        id: str,
        *,
        base: Union[str, None, NotGiven] = NOT_GIVEN,
        doc: Union[Union[DocNode, Dict[str, Any]], None, NotGiven] = NOT_GIVEN,
        markdown: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`PUT /api/v1/pages/{id}/content`"""
        return self._client._call(
            "PUT",
            f"/pages/{_p(id)}/content",
            body={"base": base, "doc": doc, "markdown": markdown},
            extra_body=extra_body,
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class PagesShareResource:
    """`client.pages.share`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def get(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Optional[ShareView]:
        """`GET /api/v1/pages/{id}/share`"""
        return self._client._call_or_none(
            "GET",
            f"/pages/{_p(id)}/share",
            cast=ShareView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def enable(
        self,
        id: str,
        *,
        include_children: Union[bool, NotGiven] = NOT_GIVEN,
        indexable: Union[bool, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> ShareView:
        """打开分享或改设置（已经分享了的，链接不变）。

        `POST /api/v1/pages/{id}/share`
        """
        return self._client._call(
            "POST",
            f"/pages/{_p(id)}/share",
            body={"include_children": include_children, "indexable": indexable},
            extra_body=extra_body,
            cast=ShareView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def disable(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """关掉分享，链接马上失效。

        `DELETE /api/v1/pages/{id}/share`
        """
        return self._client._call(
            "DELETE",
            f"/pages/{_p(id)}/share",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class PagesVersionsResource:
    """`client.pages.versions`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        id: str,
        *,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CursorPage[VersionMeta]:
        """`GET /api/v1/pages/{id}/versions`"""
        return self._client._call_page(
            "GET",
            f"/pages/{_p(id)}/versions",
            query={"limit": limit, "cursor": cursor},
            model=VersionList,
            items="versions",
            item=VersionMeta,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def create(
        self,
        id: str,
        *,
        label: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> VersionMeta:
        """`POST /api/v1/pages/{id}/versions`"""
        return self._client._call(
            "POST",
            f"/pages/{_p(id)}/versions",
            body={"label": label},
            extra_body=extra_body,
            cast=VersionMeta,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def get(
        self,
        id: str,
        v: int,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> VersionContent:
        """`GET /api/v1/pages/{id}/versions/{v}`"""
        return self._client._call(
            "GET",
            f"/pages/{_p(id)}/versions/{_p(v)}",
            cast=VersionContent,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def diff(
        self,
        id: str,
        v: int,
        *,
        to: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> VersionDiff:
        """`GET /api/v1/pages/{id}/versions/{v}/diff`"""
        return self._client._call(
            "GET",
            f"/pages/{_p(id)}/versions/{_p(v)}/diff",
            query={"to": to},
            cast=VersionDiff,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def restore(
        self,
        id: str,
        v: int,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Restored:
        """`POST /api/v1/pages/{id}/versions/{v}/restore`"""
        return self._client._call(
            "POST",
            f"/pages/{_p(id)}/versions/{_p(v)}/restore",
            cast=Restored,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class PagesResource:
    """`client.pages`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    @cached_property
    def blocks(self) -> PagesBlocksResource:
        return PagesBlocksResource(self._client)

    @cached_property
    def content(self) -> PagesContentResource:
        return PagesContentResource(self._client)

    @cached_property
    def share(self) -> PagesShareResource:
        return PagesShareResource(self._client)

    @cached_property
    def versions(self) -> PagesVersionsResource:
        return PagesVersionsResource(self._client)

    def list(
        self,
        ws: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[Page]:
        """`GET /api/v1/workspaces/{ws}/pages`"""
        return self._client._call(
            "GET",
            f"/workspaces/{_p(ws)}/pages",
            unwrap="pages",
            cast=List[Page],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def create(
        self,
        ws: str,
        *,
        after: Union[str, None, NotGiven] = NOT_GIVEN,
        icon: Union[str, None, NotGiven] = NOT_GIVEN,
        parent_id: Union[str, None, NotGiven] = NOT_GIVEN,
        title: Union[str, None, NotGiven] = NOT_GIVEN,
        markdown: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Page:
        """`POST /api/v1/workspaces/{ws}/pages`"""
        page = self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/pages",
            body={"after": after, "icon": icon, "parent_id": parent_id, "title": title},
            extra_body=extra_body,
            cast=Page,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )
        if not isinstance(markdown, NotGiven):
            # 两步：先建页面，再写正文
            self._client._call(
                "PUT",
                f"/pages/{_p(page.id)}/content",
                body={"markdown": markdown},
                cast=type(None),
                extra_headers=extra_headers,
                extra_query=extra_query,
                timeout=timeout,
            )
        return page

    def get(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Page:
        """`GET /api/v1/pages/{id}`"""
        return self._client._call(
            "GET",
            f"/pages/{_p(id)}",
            cast=Page,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def update(
        self,
        id: str,
        *,
        after: Union[str, None, NotGiven] = NOT_GIVEN,
        icon: Union[str, None, NotGiven] = NOT_GIVEN,
        parent_id: Union[str, None, NotGiven] = NOT_GIVEN,
        title: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Page:
        """`PATCH /api/v1/pages/{id}`"""
        return self._client._call(
            "PATCH",
            f"/pages/{_p(id)}",
            body={"after": after, "icon": icon, "parent_id": parent_id, "title": title},
            extra_body=extra_body,
            cast=Page,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def delete(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Deleted:
        """`DELETE /api/v1/pages/{id}`"""
        return self._client._call(
            "DELETE",
            f"/pages/{_p(id)}",
            cast=Deleted,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def authors(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Dict[str, AuthorRef]:
        """`GET /api/v1/pages/{id}/authors`"""
        return self._client._call(
            "GET",
            f"/pages/{_p(id)}/authors",
            unwrap="clients",
            cast=Dict[str, AuthorRef],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class ThreadsResource:
    """`client.threads`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        id: str,
        *,
        resolved: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[ThreadView]:
        """`GET /api/v1/pages/{id}/comments`"""
        return self._client._call(
            "GET",
            f"/pages/{_p(id)}/comments",
            query={"resolved": resolved},
            unwrap="threads",
            cast=List[ThreadView],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def create(
        self,
        id: str,
        *,
        body: str,
        anchor: Union[Union[Anchor, Dict[str, Any]], None, NotGiven] = NOT_GIVEN,
        notify: Union[Notify, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> ThreadView:
        """`POST /api/v1/pages/{id}/comments`"""
        return self._client._call(
            "POST",
            f"/pages/{_p(id)}/comments",
            body={"anchor": anchor, "body": body, "notify": notify},
            extra_body=extra_body,
            cast=ThreadView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def update(
        self,
        id: str,
        *,
        resolved: bool,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> ThreadView:
        """`PATCH /api/v1/threads/{id}`"""
        return self._client._call(
            "PATCH",
            f"/threads/{_p(id)}",
            body={"resolved": resolved},
            extra_body=extra_body,
            cast=ThreadView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def reply(
        self,
        id: str,
        *,
        body: str,
        notify: Union[Notify, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CommentView:
        """`POST /api/v1/threads/{id}/comments`"""
        return self._client._call(
            "POST",
            f"/threads/{_p(id)}/comments",
            body={"body": body, "notify": notify},
            extra_body=extra_body,
            cast=CommentView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class TrashResource:
    """`client.trash`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        ws: str,
        *,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CursorPage[TrashItem]:
        """`GET /api/v1/workspaces/{ws}/trash`"""
        return self._client._call_page(
            "GET",
            f"/workspaces/{_p(ws)}/trash",
            query={"limit": limit, "cursor": cursor},
            model=TrashList,
            items="items",
            item=TrashItem,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def restore(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> RestoredPage:
        """`POST /api/v1/trash/{id}/restore`"""
        return self._client._call(
            "POST",
            f"/trash/{_p(id)}/restore",
            cast=RestoredPage,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def purge(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`DELETE /api/v1/trash/{id}`"""
        return self._client._call(
            "DELETE",
            f"/trash/{_p(id)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class WorkspacesMembersResource:
    """`client.workspaces.members`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        ws: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> MemberList:
        """`GET /api/v1/workspaces/{ws}/members`"""
        return self._client._call(
            "GET",
            f"/workspaces/{_p(ws)}/members",
            cast=MemberList,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def set_role(
        self,
        ws: str,
        user: str,
        *,
        role: str,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> MemberRole:
        """`PATCH /api/v1/workspaces/{ws}/members/{user}`"""
        return self._client._call(
            "PATCH",
            f"/workspaces/{_p(ws)}/members/{_p(user)}",
            body={"role": role},
            extra_body=extra_body,
            cast=MemberRole,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def remove(
        self,
        ws: str,
        user: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`DELETE /api/v1/workspaces/{ws}/members/{user}`"""
        return self._client._call(
            "DELETE",
            f"/workspaces/{_p(ws)}/members/{_p(user)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class WorkspacesResource:
    """`client.workspaces`"""

    def __init__(self, client: SyncAPIClient) -> None:
        self._client = client

    @cached_property
    def members(self) -> WorkspacesMembersResource:
        return WorkspacesMembersResource(self._client)

    def update(
        self,
        ws: str,
        *,
        icon: Union[str, None, NotGiven] = NOT_GIVEN,
        name: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Workspace:
        """改工作区的名字、图标（admin 以上）。

        `PATCH /api/v1/workspaces/{ws}`
        """
        return self._client._call(
            "PATCH",
            f"/workspaces/{_p(ws)}",
            body={"icon": icon, "name": name},
            extra_body=extra_body,
            cast=Workspace,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def events(
        self,
        ws: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> EventStream:
        """SSE：工作区的实时事件，事件名和数据见 webhooks（page.created、db.changed……）。慢了会收到 resync。

        `GET /api/v1/workspaces/{ws}/events`
        """
        return self._client._stream(
            f"/workspaces/{_p(ws)}/events",
            kind="workspace",
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class SyncResources(SyncAPIClient):
    """客户端上的资源和方法（生成的）；Client / AsyncClient 继承它。"""

    @cached_property
    def agent(self) -> AgentResource:
        return AgentResource(self)

    @cached_property
    def agents(self) -> AgentsResource:
        return AgentsResource(self)

    @cached_property
    def auth(self) -> AuthResource:
        return AuthResource(self)

    @cached_property
    def comments(self) -> CommentsResource:
        return CommentsResource(self)

    @cached_property
    def databases(self) -> DatabasesResource:
        return DatabasesResource(self)

    @cached_property
    def files(self) -> FilesResource:
        return FilesResource(self)

    @cached_property
    def invites(self) -> InvitesResource:
        return InvitesResource(self)

    @cached_property
    def links(self) -> LinksResource:
        return LinksResource(self)

    @cached_property
    def me(self) -> MeResource:
        return MeResource(self)

    @cached_property
    def pages(self) -> PagesResource:
        return PagesResource(self)

    @cached_property
    def threads(self) -> ThreadsResource:
        return ThreadsResource(self)

    @cached_property
    def trash(self) -> TrashResource:
        return TrashResource(self)

    @cached_property
    def workspaces(self) -> WorkspacesResource:
        return WorkspacesResource(self)

    def search(
        self,
        ws: str,
        *,
        q: str,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        kind: Union[str, NotGiven] = NOT_GIVEN,
        per: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[SearchResult]:
        """`GET /api/v1/workspaces/{ws}/search`"""
        return self._call(
            "GET",
            f"/workspaces/{_p(ws)}/search",
            query={"q": q, "limit": limit, "kind": kind, "per": per},
            unwrap="results",
            cast=List[SearchResult],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncAgentNotificationsResource:
    """`client.agent.notifications`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        *,
        after: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        wait: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AsyncPaginator[AgentNotification]:
        """Agent 取通知（长轮询）：没有新的就等 wait 秒。不给 after / cursor 就是还没处理（ack）的。只有 Agent 能调。

        `GET /api/v1/agent/notifications`
        """
        return self._client._call_page(
            "GET",
            "/agent/notifications",
            query={"after": after, "cursor": cursor, "limit": limit, "wait": wait},
            model=AgentNotes,
            items="notifications",
            item=AgentNotification,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def ack(
        self,
        *,
        ids: Union[Sequence[int], NotGiven] = NOT_GIVEN,
        upto: Union[int, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Acked:
        """`POST /api/v1/agent/notifications/ack`"""
        return await self._client._call(
            "POST",
            "/agent/notifications/ack",
            body={"ids": ids, "upto": upto},
            extra_body=extra_body,
            cast=Acked,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def stream(
        self,
        *,
        after: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AsyncEventStream:
        """SSE：先补发游标之后的（`?after=` / `?cursor=` 或 `Last-Event-ID` 头；都没有就是没处理的），再实时推。每条是 `event: notification`，`id` 是通知编号，`data` 是 AgentNotification。每 25 秒一个注释行保活。只有 Agent 能调。

        `GET /api/v1/agent/notifications/stream`
        """
        return self._client._stream(
            "/agent/notifications/stream",
            query={"after": after, "cursor": cursor},
            kind="agent",
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncAgentResource:
    """`client.agent`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    @cached_property
    def notifications(self) -> AsyncAgentNotificationsResource:
        return AsyncAgentNotificationsResource(self._client)


class AsyncAgentsResource:
    """`client.agents`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def list(
        self,
        ws: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[AgentView]:
        """`GET /api/v1/workspaces/{ws}/agents`"""
        return await self._client._call(
            "GET",
            f"/workspaces/{_p(ws)}/agents",
            unwrap="agents",
            cast=List[AgentView],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def create(
        self,
        ws: str,
        *,
        name: str,
        scope: str,
        about: Union[str, None, NotGiven] = NOT_GIVEN,
        webhook: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CreatedAgent:
        """`POST /api/v1/workspaces/{ws}/agents`"""
        return await self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/agents",
            body={"about": about, "name": name, "scope": scope, "webhook": webhook},
            extra_body=extra_body,
            cast=CreatedAgent,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def update(
        self,
        id: str,
        *,
        about: Union[str, None, NotGiven] = NOT_GIVEN,
        locale: Union[str, None, NotGiven] = NOT_GIVEN,
        name: Union[str, None, NotGiven] = NOT_GIVEN,
        scope: Union[str, None, NotGiven] = NOT_GIVEN,
        webhook: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Agent:
        """`PATCH /api/v1/agents/{id}`"""
        return await self._client._call(
            "PATCH",
            f"/agents/{_p(id)}",
            body={"about": about, "locale": locale, "name": name, "scope": scope, "webhook": webhook},
            extra_body=extra_body,
            cast=Agent,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def revoke(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Agent:
        """撤销：token 马上失效，返回撤销后的 Agent（带 revoked_at）。

        `DELETE /api/v1/agents/{id}`
        """
        return await self._client._call(
            "DELETE",
            f"/agents/{_p(id)}",
            cast=Agent,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def rotate_token(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> NewToken:
        """`POST /api/v1/agents/{id}/token`"""
        return await self._client._call(
            "POST",
            f"/agents/{_p(id)}/token",
            cast=NewToken,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def rotate_webhook_secret(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> NewSecret:
        """`POST /api/v1/agents/{id}/webhook-secret`"""
        return await self._client._call(
            "POST",
            f"/agents/{_p(id)}/webhook-secret",
            cast=NewSecret,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncAuthResource:
    """`client.auth`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def config(
        self,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AuthConfigBody:
        """`GET /api/v1/auth/config`"""
        return await self._client._call(
            "GET",
            "/auth/config",
            cast=AuthConfigBody,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def login(
        self,
        *,
        email: str,
        password: str,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Me:
        """`POST /api/v1/auth/login`"""
        return await self._client._call(
            "POST",
            "/auth/login",
            body={"email": email, "password": password},
            extra_body=extra_body,
            cast=Me,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def logout(
        self,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`POST /api/v1/auth/logout`"""
        return await self._client._call(
            "POST",
            "/auth/logout",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def signup(
        self,
        *,
        email: str,
        password: str,
        invite: Union[str, None, NotGiven] = NOT_GIVEN,
        name: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Me:
        """注册（本地登录模式）。OAD_SIGNUP：invite（默认，要凭邀请）、open、off。

        `POST /api/v1/auth/signup`
        """
        return await self._client._call(
            "POST",
            "/auth/signup",
            body={"email": email, "invite": invite, "name": name, "password": password},
            extra_body=extra_body,
            cast=Me,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncCommentsResource:
    """`client.comments`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def update(
        self,
        id: str,
        *,
        body: str,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CommentView:
        """只有作者能改。

        `PATCH /api/v1/comments/{id}`
        """
        return await self._client._call(
            "PATCH",
            f"/comments/{_p(id)}",
            body={"body": body},
            extra_body=extra_body,
            cast=CommentView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def delete(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`DELETE /api/v1/comments/{id}`"""
        return await self._client._call(
            "DELETE",
            f"/comments/{_p(id)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncDatabasesHistoryResource:
    """`client.databases.history`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        id: str,
        *,
        row: Union[str, NotGiven] = NOT_GIVEN,
        prop: Union[str, NotGiven] = NOT_GIVEN,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AsyncPaginator[DbHistoryItem]:
        """`GET /api/v1/databases/{id}/history`"""
        return self._client._call_page(
            "GET",
            f"/databases/{_p(id)}/history",
            query={"row": row, "prop": prop, "limit": limit, "cursor": cursor},
            model=DbHistory,
            items="items",
            item=DbHistoryItem,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def undo(
        self,
        id: str,
        *,
        seq: int,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbUndone:
        """`POST /api/v1/databases/{id}/history/undo`"""
        return await self._client._call(
            "POST",
            f"/databases/{_p(id)}/history/undo",
            body={"seq": seq},
            extra_body=extra_body,
            cast=DbUndone,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncDatabasesRowsResource:
    """`client.databases.rows`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def list(
        self,
        id: str,
        *,
        since: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> RowsDelta:
        """行（全量或增量）。按 Accept-Encoding 压缩：br 优先，其次 gzip（1 万行的全量压缩后 < 1MB）。

        `GET /api/v1/databases/{id}/rows`
        """
        return await self._client._call(
            "GET",
            f"/databases/{_p(id)}/rows",
            query={"since": since},
            cast=RowsDelta,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def add(
        self,
        id: str,
        *,
        rows: Sequence[Union[AddRow, Dict[str, Any]]],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AddedRows:
        """`POST /api/v1/databases/{id}/rows`"""
        return await self._client._call(
            "POST",
            f"/databases/{_p(id)}/rows",
            body={"rows": rows},
            extra_body=extra_body,
            cast=AddedRows,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def update(
        self,
        id: str,
        *,
        ops: Sequence[Union[CellOp, Dict[str, Any]]],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Patched:
        """`PATCH /api/v1/databases/{id}/rows`"""
        return await self._client._call(
            "PATCH",
            f"/databases/{_p(id)}/rows",
            body={"ops": ops},
            extra_body=extra_body,
            cast=Patched,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def delete(
        self,
        id: str,
        *,
        ids: Sequence[str],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Seq:
        """`DELETE /api/v1/databases/{id}/rows`"""
        return await self._client._call(
            "DELETE",
            f"/databases/{_p(id)}/rows",
            body={"ids": ids},
            extra_body=extra_body,
            cast=Seq,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def delete_one(
        self,
        id: str,
        row: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Seq:
        """`DELETE /api/v1/databases/{id}/rows/{row}`"""
        return await self._client._call(
            "DELETE",
            f"/databases/{_p(id)}/rows/{_p(row)}",
            cast=Seq,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncDatabasesSchemaResource:
    """`client.databases.schema`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def update(
        self,
        id: str,
        *,
        ops: Sequence[Mapping[str, Any]],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> SchemaChanged:
        """`PATCH /api/v1/databases/{id}/schema`"""
        return await self._client._call(
            "PATCH",
            f"/databases/{_p(id)}/schema",
            body={"ops": ops},
            extra_body=extra_body,
            cast=SchemaChanged,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def undo(
        self,
        id: str,
        *,
        seq: int,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> SchemaUndone:
        """`POST /api/v1/databases/{id}/schema/undo`"""
        return await self._client._call(
            "POST",
            f"/databases/{_p(id)}/schema/undo",
            body={"seq": seq},
            extra_body=extra_body,
            cast=SchemaUndone,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncDatabasesViewsResource:
    """`client.databases.views`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def create(
        self,
        id: str,
        *,
        body: Mapping[str, Any],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbView:
        """加一个视图（不给 type 是表格），返回它（带上新的 id）。

        `POST /api/v1/databases/{id}/views`
        """
        return await self._client._call(
            "POST",
            f"/databases/{_p(id)}/views",
            body=body,
            cast=DbView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def replace(
        self,
        id: str,
        *,
        body: PutViews,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[DbView]:
        """整个换掉：`[...]` 或 `{ views: [...] }`。

        `PUT /api/v1/databases/{id}/views`
        """
        return await self._client._call(
            "PUT",
            f"/databases/{_p(id)}/views",
            body=body,
            unwrap="views",
            cast=List[DbView],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def update(
        self,
        id: str,
        view: str,
        *,
        body: Mapping[str, Any],
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbView:
        """改一个视图的几项：给了的字段覆盖，null 去掉。返回改好的视图。

        `PATCH /api/v1/databases/{id}/views/{view}`
        """
        return await self._client._call(
            "PATCH",
            f"/databases/{_p(id)}/views/{_p(view)}",
            body=body,
            cast=DbView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def delete(
        self,
        id: str,
        view: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """删一个视图（最后一个不能删）。

        `DELETE /api/v1/databases/{id}/views/{view}`
        """
        return await self._client._call(
            "DELETE",
            f"/databases/{_p(id)}/views/{_p(view)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncDatabasesResource:
    """`client.databases`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    @cached_property
    def history(self) -> AsyncDatabasesHistoryResource:
        return AsyncDatabasesHistoryResource(self._client)

    @cached_property
    def rows(self) -> AsyncDatabasesRowsResource:
        return AsyncDatabasesRowsResource(self._client)

    @cached_property
    def schema(self) -> AsyncDatabasesSchemaResource:
        return AsyncDatabasesSchemaResource(self._client)

    @cached_property
    def views(self) -> AsyncDatabasesViewsResource:
        return AsyncDatabasesViewsResource(self._client)

    async def create(
        self,
        ws: str,
        *,
        after: Union[str, None, NotGiven] = NOT_GIVEN,
        parent_id: Union[str, None, NotGiven] = NOT_GIVEN,
        schema: Union[Mapping[str, Any], None, NotGiven] = NOT_GIVEN,
        title: Union[str, None, NotGiven] = NOT_GIVEN,
        views: Union[Sequence[Mapping[str, Any]], None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbInfo:
        """`POST /api/v1/workspaces/{ws}/databases`"""
        return await self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/databases",
            body={"after": after, "parent_id": parent_id, "schema": schema, "title": title, "views": views},
            extra_body=extra_body,
            cast=DbInfo,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def import_csv(
        self,
        ws: str,
        csv: Union[str, bytes],
        *,
        title: Union[str, NotGiven] = NOT_GIVEN,
        parent_id: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbInfo:
        """导入 CSV：multipart 的 `file`（和可选的 `title`、`parent_id`），或者原始正文（`?title=`）。

        `POST /api/v1/workspaces/{ws}/databases/import`
        """
        return await self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/databases/import",
            query={"title": title, "parent_id": parent_id},
            content=_csv_body(csv),
            headers={"content-type": "text/csv; charset=utf-8"},
            cast=DbInfo,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def get(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> DbInfo:
        """库的页面、表结构、视图、现在的 seq。

        `GET /api/v1/databases/{id}`
        """
        return await self._client._call(
            "GET",
            f"/databases/{_p(id)}",
            cast=DbInfo,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def query(
        self,
        id: str,
        *,
        filter: Union[Mapping[str, Any], None, NotGiven] = NOT_GIVEN,
        group_by: Union[str, None, NotGiven] = NOT_GIVEN,
        limit: Union[int, None, NotGiven] = NOT_GIVEN,
        offset: Union[int, None, NotGiven] = NOT_GIVEN,
        search: Union[str, None, NotGiven] = NOT_GIVEN,
        sorts: Union[Sequence[Mapping[str, Any]], None, NotGiven] = NOT_GIVEN,
        totals: Union[Mapping[str, Any], None, NotGiven] = NOT_GIVEN,
        view_id: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> QueryResult:
        """服务器算的查询：给了 view_id 就从那个视图出发，别的字段覆盖它。

        `POST /api/v1/databases/{id}/query`
        """
        return await self._client._call(
            "POST",
            f"/databases/{_p(id)}/query",
            body={"filter": filter, "group_by": group_by, "limit": limit, "offset": offset, "search": search, "sorts": sorts, "totals": totals, "view_id": view_id},
            extra_body=extra_body,
            cast=QueryResult,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def export_csv(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> bytes:
        """导出 CSV（UTF-8，按第一个视图的列）。

        `GET /api/v1/databases/{id}/export.csv`
        """
        return await self._client._call(
            "GET",
            f"/databases/{_p(id)}/export.csv",
            cast=bytes,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def press_button(
        self,
        id: str,
        row: str,
        key: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Pressed:
        """按一行上的按钮：按设置改属性，有 `comment` 的在这一行上发一条评论（可以叫 Agent）。

        `POST /api/v1/databases/{id}/button/{row}/{key}`
        """
        return await self._client._call(
            "POST",
            f"/databases/{_p(id)}/button/{_p(row)}/{_p(key)}",
            cast=Pressed,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncFilesResource:
    """`client.files`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def upload(
        self,
        ws: str,
        file: FileTypes,
        filename: Optional[str] = None,
        content_type: Optional[str] = None,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Uploaded:
        """上传文件：`multipart/form-data` 的 `file` 字段，或者原始正文 + `X-Filename` 头（文件名按网址规则编码）。大小上限见 OAD_MAX_UPLOAD。图片会算宽高和 blurhash，音视频算时长。

        `POST /api/v1/workspaces/{ws}/files`
        """
        return await self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/files",
            files=_file_part(file, filename, content_type),
            cast=Uploaded,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def download(
        self,
        id: str,
        *,
        w: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> bytes:
        """同 `/files/{id}/{name}`，不带文件名。

        `GET /api/v1/files/{id}`
        """
        return await self._client._call(
            "GET",
            f"/files/{_p(id)}",
            query={"w": w},
            cast=bytes,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def download_named(
        self,
        id: str,
        name: str,
        *,
        w: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> bytes:
        """下载上传的文件：原来的类型；带 ETag、长缓存、Range。`?w=` 给图片的缩略图（WebP）。SVG、HTML 之类带 `Content-Security-Policy: sandbox`，SVG 当附件下载。不是这个工作区的成员当不存在（404）。

        `GET /api/v1/files/{id}/{name}`
        """
        return await self._client._call(
            "GET",
            f"/files/{_p(id)}/{_p(name)}",
            query={"w": w},
            cast=bytes,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def media(
        self,
        hash: str,
        *,
        w: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> bytes:
        """转存的外部图片（链接卡片的封面、头像之类），按内容的 hash。登录了就能看。`?w=` 给缩略图（WebP），支持 Range。

        `GET /api/v1/media/{hash}`
        """
        return await self._client._call(
            "GET",
            f"/media/{_p(hash)}",
            query={"w": w},
            cast=bytes,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncInvitesResource:
    """`client.invites`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def get(
        self,
        token: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> InviteInfo:
        """打开邀请链接时看的：哪个工作区、什么角色、谁邀请的。不用登录。

        `GET /api/v1/invites/{token}`
        """
        return await self._client._call(
            "GET",
            f"/invites/{_p(token)}",
            cast=InviteInfo,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def accept(
        self,
        token: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Me:
        """接受邀请，加入工作区（已经在里面的不降角色）。返回加入后的 /me。

        `POST /api/v1/invites/{token}/accept`
        """
        return await self._client._call(
            "POST",
            f"/invites/{_p(token)}/accept",
            cast=Me,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def list(
        self,
        ws: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[InviteView]:
        """`GET /api/v1/workspaces/{ws}/invites`"""
        return await self._client._call(
            "GET",
            f"/workspaces/{_p(ws)}/invites",
            unwrap="invites",
            cast=List[InviteView],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def create(
        self,
        ws: str,
        *,
        role: str,
        expires_in: Union[int, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> InviteView:
        """`POST /api/v1/workspaces/{ws}/invites`"""
        return await self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/invites",
            body={"expires_in": expires_in, "role": role},
            extra_body=extra_body,
            cast=InviteView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def revoke(
        self,
        ws: str,
        token: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`DELETE /api/v1/workspaces/{ws}/invites/{token}`"""
        return await self._client._call(
            "DELETE",
            f"/workspaces/{_p(ws)}/invites/{_p(token)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncLinksResource:
    """`client.links`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def unfurl(
        self,
        *,
        url: str,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> UnfurlCard:
        """解析一个网址成卡片：上传的文件直接从库里给，本站的库给视图列表，别的网址去抓（有缓存，图片转存到 /media）。

        `GET /api/v1/unfurl`
        """
        return await self._client._call(
            "GET",
            "/unfurl",
            query={"url": url},
            cast=UnfurlCard,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncMeNotificationsResource:
    """`client.me.notifications`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        *,
        after: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AsyncPaginator[InboxNotification]:
        """`GET /api/v1/me/notifications`"""
        return self._client._call_page(
            "GET",
            "/me/notifications",
            query={"after": after, "cursor": cursor, "limit": limit},
            model=Inbox,
            items="notifications",
            item=InboxNotification,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def mark_read(
        self,
        *,
        all: Union[bool, NotGiven] = NOT_GIVEN,
        ids: Union[Sequence[int], None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> ReadCount:
        """`POST /api/v1/me/notifications/read`"""
        return await self._client._call(
            "POST",
            "/me/notifications/read",
            body={"all": all, "ids": ids},
            extra_body=extra_body,
            cast=ReadCount,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncMeResource:
    """`client.me`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    @cached_property
    def notifications(self) -> AsyncMeNotificationsResource:
        return AsyncMeNotificationsResource(self._client)

    async def get(
        self,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Me:
        """`GET /api/v1/me`"""
        return await self._client._call(
            "GET",
            "/me",
            cast=Me,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def update(
        self,
        *,
        avatar: Union[str, None, NotGiven] = NOT_GIVEN,
        locale: Union[str, None, NotGiven] = NOT_GIVEN,
        name: Union[str, None, NotGiven] = NOT_GIVEN,
        timezone: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Me:
        """`PATCH /api/v1/me`"""
        return await self._client._call(
            "PATCH",
            "/me",
            body={"avatar": avatar, "locale": locale, "name": name, "timezone": timezone},
            extra_body=extra_body,
            cast=Me,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def change_password(
        self,
        *,
        current: str,
        new: str,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """改密码（本地登录）。别的会话都退出，这次请求用的会话保留。

        `POST /api/v1/me/password`
        """
        return await self._client._call(
            "POST",
            "/me/password",
            body={"current": current, "new": new},
            extra_body=extra_body,
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncPagesBlocksResource:
    """`client.pages.blocks`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def insert(
        self,
        id: str,
        *,
        markdown: str,
        after: Union[str, None, NotGiven] = NOT_GIVEN,
        base: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> BlockIds:
        """`POST /api/v1/pages/{id}/blocks`"""
        return await self._client._call(
            "POST",
            f"/pages/{_p(id)}/blocks",
            body={"after": after, "base": base, "markdown": markdown},
            extra_body=extra_body,
            cast=BlockIds,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def get(
        self,
        id: str,
        block_id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> BlockContent:
        """`GET /api/v1/pages/{id}/blocks/{block_id}`"""
        return await self._client._call(
            "GET",
            f"/pages/{_p(id)}/blocks/{_p(block_id)}",
            cast=BlockContent,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def replace(
        self,
        id: str,
        block_id: str,
        *,
        markdown: str,
        after: Union[str, None, NotGiven] = NOT_GIVEN,
        base: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> BlockIds:
        """`PUT /api/v1/pages/{id}/blocks/{block_id}`"""
        return await self._client._call(
            "PUT",
            f"/pages/{_p(id)}/blocks/{_p(block_id)}",
            body={"after": after, "base": base, "markdown": markdown},
            extra_body=extra_body,
            cast=BlockIds,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def delete(
        self,
        id: str,
        block_id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`DELETE /api/v1/pages/{id}/blocks/{block_id}`"""
        return await self._client._call(
            "DELETE",
            f"/pages/{_p(id)}/blocks/{_p(block_id)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncPagesContentResource:
    """`client.pages.content`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def get(
        self,
        id: str,
        *,
        format: Union[str, NotGiven] = NOT_GIVEN,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Content:
        """`GET /api/v1/pages/{id}/content`"""
        return await self._client._call(
            "GET",
            f"/pages/{_p(id)}/content",
            query={"format": format, "limit": limit},
            cast=Content,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def replace(
        self,
        id: str,
        *,
        base: Union[str, None, NotGiven] = NOT_GIVEN,
        doc: Union[Union[DocNode, Dict[str, Any]], None, NotGiven] = NOT_GIVEN,
        markdown: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`PUT /api/v1/pages/{id}/content`"""
        return await self._client._call(
            "PUT",
            f"/pages/{_p(id)}/content",
            body={"base": base, "doc": doc, "markdown": markdown},
            extra_body=extra_body,
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncPagesShareResource:
    """`client.pages.share`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def get(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Optional[ShareView]:
        """`GET /api/v1/pages/{id}/share`"""
        return await self._client._call_or_none(
            "GET",
            f"/pages/{_p(id)}/share",
            cast=ShareView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def enable(
        self,
        id: str,
        *,
        include_children: Union[bool, NotGiven] = NOT_GIVEN,
        indexable: Union[bool, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> ShareView:
        """打开分享或改设置（已经分享了的，链接不变）。

        `POST /api/v1/pages/{id}/share`
        """
        return await self._client._call(
            "POST",
            f"/pages/{_p(id)}/share",
            body={"include_children": include_children, "indexable": indexable},
            extra_body=extra_body,
            cast=ShareView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def disable(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """关掉分享，链接马上失效。

        `DELETE /api/v1/pages/{id}/share`
        """
        return await self._client._call(
            "DELETE",
            f"/pages/{_p(id)}/share",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncPagesVersionsResource:
    """`client.pages.versions`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        id: str,
        *,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AsyncPaginator[VersionMeta]:
        """`GET /api/v1/pages/{id}/versions`"""
        return self._client._call_page(
            "GET",
            f"/pages/{_p(id)}/versions",
            query={"limit": limit, "cursor": cursor},
            model=VersionList,
            items="versions",
            item=VersionMeta,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def create(
        self,
        id: str,
        *,
        label: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> VersionMeta:
        """`POST /api/v1/pages/{id}/versions`"""
        return await self._client._call(
            "POST",
            f"/pages/{_p(id)}/versions",
            body={"label": label},
            extra_body=extra_body,
            cast=VersionMeta,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def get(
        self,
        id: str,
        v: int,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> VersionContent:
        """`GET /api/v1/pages/{id}/versions/{v}`"""
        return await self._client._call(
            "GET",
            f"/pages/{_p(id)}/versions/{_p(v)}",
            cast=VersionContent,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def diff(
        self,
        id: str,
        v: int,
        *,
        to: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> VersionDiff:
        """`GET /api/v1/pages/{id}/versions/{v}/diff`"""
        return await self._client._call(
            "GET",
            f"/pages/{_p(id)}/versions/{_p(v)}/diff",
            query={"to": to},
            cast=VersionDiff,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def restore(
        self,
        id: str,
        v: int,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Restored:
        """`POST /api/v1/pages/{id}/versions/{v}/restore`"""
        return await self._client._call(
            "POST",
            f"/pages/{_p(id)}/versions/{_p(v)}/restore",
            cast=Restored,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncPagesResource:
    """`client.pages`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    @cached_property
    def blocks(self) -> AsyncPagesBlocksResource:
        return AsyncPagesBlocksResource(self._client)

    @cached_property
    def content(self) -> AsyncPagesContentResource:
        return AsyncPagesContentResource(self._client)

    @cached_property
    def share(self) -> AsyncPagesShareResource:
        return AsyncPagesShareResource(self._client)

    @cached_property
    def versions(self) -> AsyncPagesVersionsResource:
        return AsyncPagesVersionsResource(self._client)

    async def list(
        self,
        ws: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[Page]:
        """`GET /api/v1/workspaces/{ws}/pages`"""
        return await self._client._call(
            "GET",
            f"/workspaces/{_p(ws)}/pages",
            unwrap="pages",
            cast=List[Page],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def create(
        self,
        ws: str,
        *,
        after: Union[str, None, NotGiven] = NOT_GIVEN,
        icon: Union[str, None, NotGiven] = NOT_GIVEN,
        parent_id: Union[str, None, NotGiven] = NOT_GIVEN,
        title: Union[str, None, NotGiven] = NOT_GIVEN,
        markdown: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Page:
        """`POST /api/v1/workspaces/{ws}/pages`"""
        page = await self._client._call(
            "POST",
            f"/workspaces/{_p(ws)}/pages",
            body={"after": after, "icon": icon, "parent_id": parent_id, "title": title},
            extra_body=extra_body,
            cast=Page,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )
        if not isinstance(markdown, NotGiven):
            # 两步：先建页面，再写正文
            await self._client._call(
                "PUT",
                f"/pages/{_p(page.id)}/content",
                body={"markdown": markdown},
                cast=type(None),
                extra_headers=extra_headers,
                extra_query=extra_query,
                timeout=timeout,
            )
        return page

    async def get(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Page:
        """`GET /api/v1/pages/{id}`"""
        return await self._client._call(
            "GET",
            f"/pages/{_p(id)}",
            cast=Page,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def update(
        self,
        id: str,
        *,
        after: Union[str, None, NotGiven] = NOT_GIVEN,
        icon: Union[str, None, NotGiven] = NOT_GIVEN,
        parent_id: Union[str, None, NotGiven] = NOT_GIVEN,
        title: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Page:
        """`PATCH /api/v1/pages/{id}`"""
        return await self._client._call(
            "PATCH",
            f"/pages/{_p(id)}",
            body={"after": after, "icon": icon, "parent_id": parent_id, "title": title},
            extra_body=extra_body,
            cast=Page,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def delete(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Deleted:
        """`DELETE /api/v1/pages/{id}`"""
        return await self._client._call(
            "DELETE",
            f"/pages/{_p(id)}",
            cast=Deleted,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def authors(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Dict[str, AuthorRef]:
        """`GET /api/v1/pages/{id}/authors`"""
        return await self._client._call(
            "GET",
            f"/pages/{_p(id)}/authors",
            unwrap="clients",
            cast=Dict[str, AuthorRef],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncThreadsResource:
    """`client.threads`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def list(
        self,
        id: str,
        *,
        resolved: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[ThreadView]:
        """`GET /api/v1/pages/{id}/comments`"""
        return await self._client._call(
            "GET",
            f"/pages/{_p(id)}/comments",
            query={"resolved": resolved},
            unwrap="threads",
            cast=List[ThreadView],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def create(
        self,
        id: str,
        *,
        body: str,
        anchor: Union[Union[Anchor, Dict[str, Any]], None, NotGiven] = NOT_GIVEN,
        notify: Union[Notify, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> ThreadView:
        """`POST /api/v1/pages/{id}/comments`"""
        return await self._client._call(
            "POST",
            f"/pages/{_p(id)}/comments",
            body={"anchor": anchor, "body": body, "notify": notify},
            extra_body=extra_body,
            cast=ThreadView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def update(
        self,
        id: str,
        *,
        resolved: bool,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> ThreadView:
        """`PATCH /api/v1/threads/{id}`"""
        return await self._client._call(
            "PATCH",
            f"/threads/{_p(id)}",
            body={"resolved": resolved},
            extra_body=extra_body,
            cast=ThreadView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def reply(
        self,
        id: str,
        *,
        body: str,
        notify: Union[Notify, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> CommentView:
        """`POST /api/v1/threads/{id}/comments`"""
        return await self._client._call(
            "POST",
            f"/threads/{_p(id)}/comments",
            body={"body": body, "notify": notify},
            extra_body=extra_body,
            cast=CommentView,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncTrashResource:
    """`client.trash`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    def list(
        self,
        ws: str,
        *,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        cursor: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AsyncPaginator[TrashItem]:
        """`GET /api/v1/workspaces/{ws}/trash`"""
        return self._client._call_page(
            "GET",
            f"/workspaces/{_p(ws)}/trash",
            query={"limit": limit, "cursor": cursor},
            model=TrashList,
            items="items",
            item=TrashItem,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def restore(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> RestoredPage:
        """`POST /api/v1/trash/{id}/restore`"""
        return await self._client._call(
            "POST",
            f"/trash/{_p(id)}/restore",
            cast=RestoredPage,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def purge(
        self,
        id: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`DELETE /api/v1/trash/{id}`"""
        return await self._client._call(
            "DELETE",
            f"/trash/{_p(id)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncWorkspacesMembersResource:
    """`client.workspaces.members`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    async def list(
        self,
        ws: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> MemberList:
        """`GET /api/v1/workspaces/{ws}/members`"""
        return await self._client._call(
            "GET",
            f"/workspaces/{_p(ws)}/members",
            cast=MemberList,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def set_role(
        self,
        ws: str,
        user: str,
        *,
        role: str,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> MemberRole:
        """`PATCH /api/v1/workspaces/{ws}/members/{user}`"""
        return await self._client._call(
            "PATCH",
            f"/workspaces/{_p(ws)}/members/{_p(user)}",
            body={"role": role},
            extra_body=extra_body,
            cast=MemberRole,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    async def remove(
        self,
        ws: str,
        user: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> None:
        """`DELETE /api/v1/workspaces/{ws}/members/{user}`"""
        return await self._client._call(
            "DELETE",
            f"/workspaces/{_p(ws)}/members/{_p(user)}",
            cast=type(None),
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncWorkspacesResource:
    """`client.workspaces`"""

    def __init__(self, client: AsyncAPIClient) -> None:
        self._client = client

    @cached_property
    def members(self) -> AsyncWorkspacesMembersResource:
        return AsyncWorkspacesMembersResource(self._client)

    async def update(
        self,
        ws: str,
        *,
        icon: Union[str, None, NotGiven] = NOT_GIVEN,
        name: Union[str, None, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        extra_body: Optional[Body] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> Workspace:
        """改工作区的名字、图标（admin 以上）。

        `PATCH /api/v1/workspaces/{ws}`
        """
        return await self._client._call(
            "PATCH",
            f"/workspaces/{_p(ws)}",
            body={"icon": icon, "name": name},
            extra_body=extra_body,
            cast=Workspace,
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )

    def events(
        self,
        ws: str,
        *,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> AsyncEventStream:
        """SSE：工作区的实时事件，事件名和数据见 webhooks（page.created、db.changed……）。慢了会收到 resync。

        `GET /api/v1/workspaces/{ws}/events`
        """
        return self._client._stream(
            f"/workspaces/{_p(ws)}/events",
            kind="workspace",
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )


class AsyncResources(AsyncAPIClient):
    """客户端上的资源和方法（生成的）；Client / AsyncClient 继承它。"""

    @cached_property
    def agent(self) -> AsyncAgentResource:
        return AsyncAgentResource(self)

    @cached_property
    def agents(self) -> AsyncAgentsResource:
        return AsyncAgentsResource(self)

    @cached_property
    def auth(self) -> AsyncAuthResource:
        return AsyncAuthResource(self)

    @cached_property
    def comments(self) -> AsyncCommentsResource:
        return AsyncCommentsResource(self)

    @cached_property
    def databases(self) -> AsyncDatabasesResource:
        return AsyncDatabasesResource(self)

    @cached_property
    def files(self) -> AsyncFilesResource:
        return AsyncFilesResource(self)

    @cached_property
    def invites(self) -> AsyncInvitesResource:
        return AsyncInvitesResource(self)

    @cached_property
    def links(self) -> AsyncLinksResource:
        return AsyncLinksResource(self)

    @cached_property
    def me(self) -> AsyncMeResource:
        return AsyncMeResource(self)

    @cached_property
    def pages(self) -> AsyncPagesResource:
        return AsyncPagesResource(self)

    @cached_property
    def threads(self) -> AsyncThreadsResource:
        return AsyncThreadsResource(self)

    @cached_property
    def trash(self) -> AsyncTrashResource:
        return AsyncTrashResource(self)

    @cached_property
    def workspaces(self) -> AsyncWorkspacesResource:
        return AsyncWorkspacesResource(self)

    async def search(
        self,
        ws: str,
        *,
        q: str,
        limit: Union[int, NotGiven] = NOT_GIVEN,
        kind: Union[str, NotGiven] = NOT_GIVEN,
        per: Union[str, NotGiven] = NOT_GIVEN,
        extra_headers: Optional[Headers] = None,
        extra_query: Optional[Query] = None,
        timeout: Union[TimeoutTypes, NotGiven] = NOT_GIVEN,
    ) -> List[SearchResult]:
        """`GET /api/v1/workspaces/{ws}/search`"""
        return await self._call(
            "GET",
            f"/workspaces/{_p(ws)}/search",
            query={"q": q, "limit": limit, "kind": kind, "per": per},
            unwrap="results",
            cast=List[SearchResult],
            extra_headers=extra_headers,
            extra_query=extra_query,
            timeout=timeout,
        )
