// 由 scripts/gen-types.mjs 从 docs/openapi.json 生成，不要手改（pnpm --filter @open-agent-docs/client gen:types）
export interface paths {
    "/agent/notifications": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 长轮询：`?after=<id>&wait=<秒，最多 60>&limit=<最多 100>`。
         * @description Agent 取通知（长轮询）：没有新的就等 wait 秒。不给 after / cursor 就是还没处理（ack）的。只有 Agent 能调。
         */
        get: operations["poll_notes"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/agent/notifications/ack": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /agent/notifications/ack */
        post: operations["ack_notes"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/agent/notifications/stream": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * SSE：先补发游标之后的（`?after=` 或 `Last-Event-ID`；都没有就是没处理的），再实时推。每 25 秒一个注释行保活。
         * @description SSE：先补发游标之后的（`?after=` / `?cursor=` 或 `Last-Event-ID` 头；都没有就是没处理的），再实时推。每条是 `event: notification`，`id` 是通知编号，`data` 是 AgentNotification。每 25 秒一个注释行保活。只有 Agent 能调。
         */
        get: operations["stream_notes"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/agents/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /**
         * 撤销：token 马上失效，返回撤销后的 Agent（带 revoked_at）。
         * @description 撤销：token 马上失效，返回撤销后的 Agent（带 revoked_at）。
         */
        delete: operations["revoke_agent"];
        options?: never;
        head?: never;
        /** PATCH /agents/{id} */
        patch: operations["update_agent"];
        trace?: never;
    };
    "/agents/{id}/token": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /agents/{id}/token */
        post: operations["rotate_token"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/agents/{id}/webhook-secret": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /agents/{id}/webhook-secret */
        post: operations["rotate_secret"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/auth/config": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /auth/config */
        get: operations["auth_config"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/auth/login": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /auth/login */
        post: operations["login"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/auth/logout": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /auth/logout */
        post: operations["logout"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/auth/signup": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 注册（本地登录模式）。OAD_SIGNUP：invite（默认，要凭邀请）、open、off。
         * @description 注册（本地登录模式）。OAD_SIGNUP：invite（默认，要凭邀请）、open、off。
         */
        post: operations["signup"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/comments/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** DELETE /comments/{id} */
        delete: operations["delete_comment"];
        options?: never;
        head?: never;
        /**
         * 只有作者能改。
         * @description 只有作者能改。
         */
        patch: operations["edit_comment"];
        trace?: never;
    };
    "/databases/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** 库的页面、表结构、视图、现在的 seq。 */
        get: operations["info"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/databases/{id}/button/{row}/{key}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** 按一行上的按钮：按设置改属性，有 `comment` 的在这一行上发一条评论（可以叫 Agent）。 */
        post: operations["press"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/databases/{id}/export.csv": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** 导出 CSV（UTF-8，按第一个视图的列）。 */
        get: operations["export"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/databases/{id}/history": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /databases/{id}/history */
        get: operations["db_history"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/databases/{id}/history/undo": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /databases/{id}/history/undo */
        post: operations["db_undo"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/databases/{id}/query": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** 服务器算的查询：给了 view_id 就从那个视图出发，别的字段覆盖它。 */
        post: operations["query"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/databases/{id}/rows": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** 行（全量或增量）。按 Accept-Encoding 压缩：br 优先，其次 gzip（1 万行的全量压缩后 < 1MB）。 */
        get: operations["rows"];
        put?: never;
        /** POST /databases/{id}/rows */
        post: operations["add_rows"];
        /** DELETE /databases/{id}/rows */
        delete: operations["delete_rows"];
        options?: never;
        head?: never;
        /** PATCH /databases/{id}/rows */
        patch: operations["patch_rows"];
        trace?: never;
    };
    "/databases/{id}/rows/{row}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** DELETE /databases/{id}/rows/{row} */
        delete: operations["delete_one"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/databases/{id}/schema": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /** PATCH /databases/{id}/schema */
        patch: operations["change_schema"];
        trace?: never;
    };
    "/databases/{id}/schema/undo": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /databases/{id}/schema/undo */
        post: operations["undo_schema"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/databases/{id}/views": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** 整个换掉：`[...]` 或 `{ views: [...] }`。 */
        put: operations["put_views"];
        /** 加一个视图（不给 type 是表格），返回它（带上新的 id）。 */
        post: operations["add_view"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/databases/{id}/views/{view}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** 删一个视图（最后一个不能删）。 */
        delete: operations["delete_view"];
        options?: never;
        head?: never;
        /** 改一个视图的几项：给了的字段覆盖，null 去掉。返回改好的视图。 */
        patch: operations["patch_view"];
        trace?: never;
    };
    "/files/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 同 `/files/{id}/{name}`，不带文件名。
         * @description 同 `/files/{id}/{name}`，不带文件名。
         */
        get: operations["get_file_bare"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/files/{id}/{name}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 下载上传的文件：原来的类型；带 ETag、长缓存、Range。`?w=` 给图片的缩略图（WebP）。SVG、HTML 之类带 `Content-Security-Policy: sandbox`，SVG 当附件下载。不是这个工作区的成员当不存在（404）。
         * @description 下载上传的文件：原来的类型；带 ETag、长缓存、Range。`?w=` 给图片的缩略图（WebP）。SVG、HTML 之类带 `Content-Security-Policy: sandbox`，SVG 当附件下载。不是这个工作区的成员当不存在（404）。
         */
        get: operations["get_file"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/invites/{token}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 打开邀请链接时看的：哪个工作区、什么角色、谁邀请的。不用登录。
         * @description 打开邀请链接时看的：哪个工作区、什么角色、谁邀请的。不用登录。
         */
        get: operations["invite_info"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/invites/{token}/accept": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 接受邀请，加入工作区（已经在里面的不降角色）。返回加入后的 /me。
         * @description 接受邀请，加入工作区（已经在里面的不降角色）。返回加入后的 /me。
         */
        post: operations["accept"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /me */
        get: operations["me"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /** PATCH /me */
        patch: operations["update_me"];
        trace?: never;
    };
    "/me/notifications": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /me/notifications */
        get: operations["my_notes"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/me/notifications/read": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /me/notifications/read */
        post: operations["read_my_notes"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/me/password": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 改密码（本地登录）。别的会话都退出，这次请求用的会话保留。
         * @description 改密码（本地登录）。别的会话都退出，这次请求用的会话保留。
         */
        post: operations["change_password"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/media/{hash}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 转存的外部图片（链接卡片的封面、头像之类），按内容的 hash。登录了就能看。`?w=` 给缩略图（WebP），支持 Range。
         * @description 转存的外部图片（链接卡片的封面、头像之类），按内容的 hash。登录了就能看。`?w=` 给缩略图（WebP），支持 Range。
         */
        get: operations["get_media"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/pages/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /pages/{id} */
        get: operations["get_page"];
        put?: never;
        post?: never;
        /** DELETE /pages/{id} */
        delete: operations["delete_page"];
        options?: never;
        head?: never;
        /** PATCH /pages/{id} */
        patch: operations["patch_page"];
        trace?: never;
    };
    "/pages/{id}/authors": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /pages/{id}/authors */
        get: operations["authors"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/pages/{id}/blocks": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /pages/{id}/blocks */
        post: operations["insert_blocks"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/pages/{id}/blocks/{block_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /pages/{id}/blocks/{block_id} */
        get: operations["get_block"];
        /** PUT /pages/{id}/blocks/{block_id} */
        put: operations["put_block"];
        post?: never;
        /** DELETE /pages/{id}/blocks/{block_id} */
        delete: operations["delete_block"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/pages/{id}/comments": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /pages/{id}/comments */
        get: operations["list_comments"];
        put?: never;
        /** POST /pages/{id}/comments */
        post: operations["create_thread"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/pages/{id}/content": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /pages/{id}/content */
        get: operations["get_content"];
        /** PUT /pages/{id}/content */
        put: operations["put_content"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/pages/{id}/share": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /pages/{id}/share */
        get: operations["get_share"];
        put?: never;
        /**
         * 打开分享或改设置（已经分享了的，链接不变）。
         * @description 打开分享或改设置（已经分享了的，链接不变）。
         */
        post: operations["put_share"];
        /**
         * 关掉分享，链接马上失效。
         * @description 关掉分享，链接马上失效。
         */
        delete: operations["del_share"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/pages/{id}/versions": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /pages/{id}/versions */
        get: operations["history_list"];
        put?: never;
        /** POST /pages/{id}/versions */
        post: operations["save"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/pages/{id}/versions/{v}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /pages/{id}/versions/{v} */
        get: operations["one"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/pages/{id}/versions/{v}/diff": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /pages/{id}/versions/{v}/diff */
        get: operations["diff"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/pages/{id}/versions/{v}/restore": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /pages/{id}/versions/{v}/restore */
        post: operations["history_restore"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/sync/{page_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * WebSocket（y-websocket 协议）：协同编辑。没权限时升级后发 permission-denied 再关。
         * @description WebSocket（y-websocket 协议）：协同编辑。没权限时升级后发 permission-denied 再关。
         */
        get: operations["handler"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/threads/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /** PATCH /threads/{id} */
        patch: operations["patch_thread"];
        trace?: never;
    };
    "/threads/{id}/comments": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /threads/{id}/comments */
        post: operations["reply"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/trash/{id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** DELETE /trash/{id} */
        delete: operations["purge"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/trash/{id}/restore": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /trash/{id}/restore */
        post: operations["trash_restore"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/unfurl": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * 解析一个网址成卡片：上传的文件直接从库里给，本站的库给视图列表，别的网址去抓（有缓存，图片转存到 /media）。
         * @description 解析一个网址成卡片：上传的文件直接从库里给，本站的库给视图列表，别的网址去抓（有缓存，图片转存到 /media）。
         */
        get: operations["unfurl"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/workspaces/{ws}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        /**
         * 改工作区的名字、图标（admin 以上）。
         * @description 改工作区的名字、图标（admin 以上）。
         */
        patch: operations["update_ws"];
        trace?: never;
    };
    "/workspaces/{ws}/agents": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /workspaces/{ws}/agents */
        get: operations["list_agents"];
        put?: never;
        /** POST /workspaces/{ws}/agents */
        post: operations["create_agent"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/workspaces/{ws}/databases": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** POST /workspaces/{ws}/databases */
        post: operations["create"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/workspaces/{ws}/databases/import": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** 导入 CSV：multipart 的 `file`（和可选的 `title`、`parent_id`），或者原始正文（`?title=`）。 */
        post: operations["import"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/workspaces/{ws}/events": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * SSE：工作区的实时事件，事件名和数据见 webhooks（page.created、db.changed……）。慢了会收到 resync。
         * @description SSE：工作区的实时事件，事件名和数据见 webhooks（page.created、db.changed……）。慢了会收到 resync。
         */
        get: operations["events"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/workspaces/{ws}/files": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /**
         * 上传：`multipart/form-data` 的 `file` 字段，或者原始正文 + `X-Filename` 头。边读边数，超过上限就停。
         * @description 上传文件：`multipart/form-data` 的 `file` 字段，或者原始正文 + `X-Filename` 头（文件名按网址规则编码）。大小上限见 OAD_MAX_UPLOAD。图片会算宽高和 blurhash，音视频算时长。
         */
        post: operations["upload"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/workspaces/{ws}/invites": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /workspaces/{ws}/invites */
        get: operations["list_invites"];
        put?: never;
        /** POST /workspaces/{ws}/invites */
        post: operations["create_invite"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/workspaces/{ws}/invites/{token}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** DELETE /workspaces/{ws}/invites/{token} */
        delete: operations["revoke_invite"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/workspaces/{ws}/members": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /workspaces/{ws}/members */
        get: operations["members"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/workspaces/{ws}/members/{user}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        /** DELETE /workspaces/{ws}/members/{user} */
        delete: operations["remove_member"];
        options?: never;
        head?: never;
        /** PATCH /workspaces/{ws}/members/{user} */
        patch: operations["set_role"];
        trace?: never;
    };
    "/workspaces/{ws}/pages": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /workspaces/{ws}/pages */
        get: operations["list_pages"];
        put?: never;
        /** POST /workspaces/{ws}/pages */
        post: operations["create_page"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/workspaces/{ws}/search": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /workspaces/{ws}/search */
        get: operations["search"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/workspaces/{ws}/trash": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** GET /workspaces/{ws}/trash */
        get: operations["trash_list"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
}
export interface webhooks {
    "agent.notification": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** Agent 的 webhook：POST 到 Agent 设的地址；头 X-OAD-Timestamp（毫秒）、X-OAD-Signature: sha256=<hex(HMAC-SHA256(webhook_secret, "<timestamp>.<body>"))>、X-OAD-Delivery（通知编号）。回 2xx 算送到，否则退避重试 */
        post: operations["agent_notification"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "comment.created": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：新评论 */
        post: operations["comment_created"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "comment.deleted": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：评论删了 */
        post: operations["comment_deleted"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "comment.updated": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：评论改了 */
        post: operations["comment_updated"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "db.changed": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：库的行变了，拉 rows?since= */
        post: operations["db_changed"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "db.schema": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：库的表结构或视图变了 */
        post: operations["db_schema"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    notification: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：给本人的通知（收件箱） */
        post: operations["notification"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "page.created": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE（GET /workspaces/{ws}/events）：新建 / 从回收站放回的页面 */
        post: operations["page_created"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "page.deleted": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：页面进了回收站（行不发） */
        post: operations["page_deleted"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "page.moved": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：页面挪了位置 */
        post: operations["page_moved"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "page.purged": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：页面彻底删了 */
        post: operations["page_purged"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "page.updated": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：页面改了标题、图标（行的标题改了也有，kind = row） */
        post: operations["page_updated"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    resync: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：落下了事件，重新拉列表 */
        post: operations["resync"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "thread.updated": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：评论串解决 / 重新打开 */
        post: operations["thread_updated"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "workspace.updated": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** SSE：工作区改了名字、图标 */
        post: operations["workspace_updated"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
}
export interface components {
    schemas: {
        /** @description 处理完了：给 ids，或者 upto（编号 ≤ upto 的都算），至少给一个。 */
        Ack: {
            /** @description 这些编号 */
            ids?: number[];
            /**
             * Format: int64
             * @description 编号 ≤ 这个的
             */
            upto?: number | null;
        };
        Acked: {
            /**
             * Format: int64
             * @description 标了几条
             */
            acked: number;
        };
        AddRow: {
            /** @description 排在这一行后面（不给放最后） */
            after?: string | null;
            /** @description 行页面的正文（Markdown） */
            body?: string | null;
            /** @description 属性：键是属性的 key 或名字，值是存的样子（选项也可以写名字） */
            props?: Record<string, unknown>;
        };
        AddRows: {
            rows: components["schemas"]["AddRow"][];
        };
        AddedRows: {
            /** @description 新行的 id（和给的顺序一样） */
            ids: string[];
            /** Format: int64 */
            seq: number;
        };
        Agent: {
            about: string;
            /** Format: int64 */
            created_at: number;
            created_by?: string | null;
            id: string;
            /** Format: int64 */
            last_used_at?: number | null;
            name: string;
            /** Format: int64 */
            revoked_at?: number | null;
            scope: components["schemas"]["Scope"];
            webhook?: string | null;
            workspace_id: string;
        };
        AgentList: {
            agents: components["schemas"]["AgentView"][];
        };
        AgentNotes: {
            /** @description 取满了一页时是最后一条的编号，带 `?cursor=` 接着取；没有就是取完了 */
            next_cursor?: string | null;
            notifications: components["schemas"]["AgentNotification"][];
        };
        /** @description Agent 收到的一条通知（webhook 的正文；长轮询 / SSE 取到的也是这样）。 */
        AgentNotification: {
            agent: components["schemas"]["NoteAgent"];
            /** @description 评论挂在哪一块上（对整页的评论是 null） */
            block_id?: string | null;
            comment: components["schemas"]["NoteComment"];
            /** Format: int64 */
            created_at: number;
            /**
             * Format: int64
             * @description 通知编号（处理完带回来当 after / cursor）
             */
            id: number;
            page: components["schemas"]["NotePage"];
            /** @description 一段可以直接当消息发给 Agent 的话（按 Agent 的语言） */
            prompt: string;
            /** @description 选中的原文 */
            quote?: string | null;
            thread_id: string;
            /** @description comment */
            type: string;
        };
        /** @description Agent 列表里的一项：加上没处理的通知数和最近的 webhook 错误。 */
        AgentView: components["schemas"]["Agent"] & {
            /** Format: int64 */
            pending: number;
            webhook_error?: string | null;
        };
        /** @description 评论挂在哪：一段文字（start / end 是网页给的 Yjs RelativePosition，服务器原样存）、整段（只有 block_id）或整页（None）。 */
        Anchor: {
            block_id?: string | null;
            end?: string | null;
            quote?: string | null;
            start?: string | null;
        };
        /** @description 登录方式和站点设置。 */
        AuthConfigBody: {
            /** @description 登录方式：local（账号密码）/ header（宿主反代带身份头）/ none（不登录，单人）/ jwt（宿主签的 JWT） */
            mode: components["schemas"]["AuthMode"];
            signup: components["schemas"]["SignupMode"];
            /** @description 服务器的时区（IANA 名字） */
            timezone: string;
        };
        /** @enum {string} */
        AuthMode: "local" | "header" | "none" | "jwt";
        AuthorRef: {
            id: string;
            kind: components["schemas"]["PrincipalKind"];
            name: string;
        };
        /** @description 每个 Yjs client id（字符串）是谁写的。 */
        Authors: {
            clients: {
                [key: string]: components["schemas"]["AuthorRef"];
            };
        };
        BlockBody: {
            after?: string | null;
            base?: string | null;
            markdown: string;
        };
        BlockContent: {
            block: components["schemas"]["DocNode"];
            markdown: string;
            version: string;
        };
        /** @description 对比里的一块。 */
        BlockDiff: {
            after?: string | null;
            before?: string | null;
            /** @description 这块里在两版之间写了字的人 */
            by?: components["schemas"]["VersionAuthor"][];
            id: string;
            /** @description added / removed / changed / same */
            op: string;
            /** @description 块的类型（paragraph、heading…） */
            type: string;
        };
        BlockIds: {
            ids: string[];
        };
        /** @description 一格的改动。 */
        CellChange: {
            new?: unknown;
            old?: unknown;
            prop: string;
            row_id: string;
        };
        /** @description 改一行的几格：值为 null 的清掉。给了 `after` 是挪位置（排在那一行后面，null 是最前面）。 */
        CellOp: {
            after?: string | null;
            row_id: string;
            set?: {
                [key: string]: unknown;
            };
        };
        CommentView: {
            author: components["schemas"]["AuthorRef"];
            /** @description 删掉的是空字符串。 */
            body: string;
            /** Format: int64 */
            created_at: number;
            /** Format: int64 */
            deleted_at?: number | null;
            /** Format: int64 */
            edited_at?: number | null;
            id: string;
            thread_id: string;
        };
        /** @description 正文：`markdown`（默认）或 `doc`（`?format=json`），带当前版本（改的时候作为 base 带回来）。 */
        Content: {
            doc?: components["schemas"]["DocNode"] | null;
            markdown?: string | null;
            /** @description 顶层块一共几个（给了 `limit` 才有） */
            total?: number | null;
            version: string;
        };
        CreateAgent: {
            /** @description 介绍（给人和别的 Agent 看） */
            about?: string | null;
            /** @description 1 到 40 个字，不能有空白和 @ [ ] ( ) < > 引号 逗号 */
            name: string;
            /** @description read / write / admin */
            scope: string;
            /** @description 收通知的 webhook 地址 */
            webhook?: string | null;
        };
        CreatePage: {
            after?: string | null;
            icon?: string | null;
            parent_id?: string | null;
            title?: string | null;
        };
        /** @description 新建的 Agent；token 和 webhook_secret 只有这一次能看到。 */
        CreatedAgent: {
            agent: components["schemas"]["Agent"];
            /** @description `oad_…`，当 Bearer token 用 */
            token: string;
            /** @description `whsec_…`，验 webhook 的签名 */
            webhook_secret: string;
        };
        /** @description 库的历史。 */
        DbHistory: {
            /** @description 新的在前；同一次改动（同一个 seq）的格子不会分在两页 */
            items: components["schemas"]["DbHistoryItem"][];
            /** @description 还有下一页时给 */
            next_cursor?: string | null;
            /**
             * Format: int64
             * @description 库现在的 seq
             */
            seq: number;
        };
        /** @description 历史里的一条。`kind`：`cell`（一格改了）、`row`（加 / 删了一行）、`schema`（表结构改了，只在不按行 / 属性筛时有）。 */
        DbHistoryItem: {
            /** Format: int64 */
            at: number;
            by?: components["schemas"]["AuthorRef"] | string | null;
            /** @description schema 有：顺带改了几格 */
            cells?: number | null;
            /** @description schema 有：这次动了哪些属性 */
            changes?: Record<string, unknown>[] | null;
            /** @description cell / row / schema */
            kind: string;
            new?: unknown;
            old?: unknown;
            /** @description cell 有：属性的 key */
            prop?: string | null;
            /** @description cell 有：属性的名字 */
            prop_name?: string | null;
            /** @description cell / row 有 */
            row_id?: string | null;
            /** @description cell / row 有 */
            row_title?: string | null;
            /** Format: int64 */
            seq: number;
            /** @description row / schema 有：一句话说明 */
            summary?: string | null;
            /** @description schema 有：撤销过了 */
            undone?: boolean | null;
        };
        /** @description 库的信息（GET /databases/{id}）。 */
        DbInfo: {
            page: components["schemas"]["Page"];
            schema: components["schemas"]["DbSchema"];
            /** Format: int64 */
            seq: number;
            views: components["schemas"]["DbView"][];
        };
        /** @description 一个属性（一列）。`key` 是短随机串，改名不动数据；各类型自己的配置只在那个类型上有。 */
        DbProp: {
            /** @description button 按下时通知的 Agent（id） */
            agent?: string;
            /** @description button 按下时发的评论 */
            comment?: string;
            /** @description 日期的显示：iso | zh | slash | relative */
            date_format?: string;
            /** @description 数字的格式：number | comma | percent | cny | usd（number、formula、rollup） */
            format?: string;
            /** @description formula 的公式 */
            formula?: string;
            key: string;
            /** @description button 的文字 */
            label?: string;
            name: string;
            /** @description select / multi_select / status 的选项 */
            options?: components["schemas"]["SelectOption"][];
            /**
             * Format: int32
             * @description 显示几位小数
             */
            precision?: number;
            /** @description unique_id 的前缀（T-12） */
            prefix?: string;
            relation?: components["schemas"]["RelationConf"];
            rollup?: components["schemas"]["RollupConf"];
            /** @description button 按下时设的值：`{ 属性 key 或名字: 值 }` */
            set?: Record<string, unknown>;
            type: components["schemas"]["PropType"];
        };
        /** @description 库里的一行。`props` 是存的样子（键是属性的 key），标题也在里面（和行页面的标题同步）。 */
        DbRow: {
            /** Format: int64 */
            created_at: number;
            created_by?: string | null;
            db_id: string;
            /** Format: int64 */
            deleted_at?: number | null;
            id: string;
            position: string;
            props: {
                [key: string]: unknown;
            };
            /** Format: int64 */
            seq: number;
            /** Format: int64 */
            unique_id: number;
            /** Format: int64 */
            updated_at: number;
            updated_by?: string | null;
        };
        /** @description 表结构。 */
        DbSchema: {
            /**
             * Format: int64
             * @description 下一个自动编号
             */
            next_id?: number;
            props: components["schemas"]["DbProp"][];
        };
        /** @description 排序：`dir: "asc" | "desc"`。 */
        DbSort: {
            dir?: string;
            prop: string;
        };
        /** @description 撤销的结果：格子改动只有 `seq`；表结构改动还有新的 `schema`、`views`。 */
        DbUndone: {
            schema?: components["schemas"]["DbSchema"] | null;
            /**
             * Format: int64
             * @description 撤销后库的 seq
             */
            seq: number;
            views?: components["schemas"]["DbView"][] | null;
        };
        /** @description 一个视图。存的时候按表结构清理（去掉指向已删属性的排序、分组、隐藏、列宽）。 */
        DbView: {
            /** @description 日历按哪个日期属性 */
            calendar_by?: string;
            /** @description 筛选：`{ op: "and" | "or", items: [{ prop, op, value } 或嵌套的组] }` */
            filter?: Record<string, unknown>;
            /** @description 按哪个属性分组（看板的列） */
            group_by?: string;
            /** @description 按日期分组时怎么分：day | week | month | year | relative */
            group_date?: string;
            /** @description 隐藏的属性 key */
            hidden?: string[];
            /** @description 去掉没有行的分组 */
            hide_empty_groups?: boolean;
            id: string;
            name: string;
            /** @description 列的顺序（属性 key） */
            order?: string[];
            sorts?: components["schemas"]["DbSort"][];
            sub_group_by?: string;
            sub_group_date?: string;
            /** @description 每列底部的汇总：`{ key: fn }` */
            totals?: Record<string, unknown>;
            type: components["schemas"]["ViewType"];
            /** @description 列宽：`{ key: 像素 }` */
            widths?: Record<string, unknown>;
        };
        Deleted: {
            /** @description 进了回收站的页面（连子页面） */
            deleted: string[];
        };
        /** @description 对比的另一头：版本号，或 `"current"`。 */
        DiffTo: number | string;
        DocMark: {
            /** @description 没有属性时不给 */
            attrs?: Record<string, unknown>;
            type: string;
        };
        /** @description 正文的节点树（和编辑器的 ProseMirror JSON 一样）：`{ type, attrs?, content?, text?, marks? }`。 */
        DocNode: {
            /** @description 没有属性时不给 */
            attrs?: Record<string, unknown>;
            content?: components["schemas"]["DocNode"][] | null;
            marks?: components["schemas"]["DocMark"][] | null;
            text?: string | null;
            type: string;
        };
        EditComment: {
            /** @description 新的正文（Markdown） */
            body: string;
        };
        /** @description 统一的错误：`{ error: { code, message } }`。message 按调用者的语言（中文 / 英文）。 */
        ErrorBody: {
            error: components["schemas"]["ErrorInfo"];
        };
        /**
         * @description 错误码：not_found 404、bad_request 400、unauthorized 401、forbidden 403、conflict 409、gone 410（邀请过期之类）、
         *     too_large 413、too_many_requests 429（带 Retry-After）、bad_gateway 502（去外部网站取东西失败）、internal 500。
         * @enum {string}
         */
        ErrorCode: "not_found" | "bad_request" | "unauthorized" | "forbidden" | "conflict" | "gone" | "too_large" | "too_many_requests" | "bad_gateway" | "internal";
        ErrorInfo: {
            code: components["schemas"]["ErrorCode"];
            /** @description 给人看的原因 */
            message: string;
        };
        Ids: {
            /** @description 要删的行（进回收站） */
            ids: string[];
        };
        /** @description multipart 导入的字段。 */
        ImportForm: {
            /** @description CSV 文件（第一行是列名） */
            file: string;
            /** @description 放在哪个页面下 */
            parent_id?: string | null;
            /** @description 库名 */
            title?: string | null;
        };
        Inbox: {
            /** @description 取满了一页时是最后一条（最旧的）的编号，带 `?cursor=` 往旧的翻；没有就是到头了 */
            next_cursor?: string | null;
            /** @description 新的在前 */
            notifications: components["schemas"]["InboxNotification"][];
            /**
             * Format: int64
             * @description 没读的总数
             */
            unread: number;
        };
        /** @description 收件箱里的一条。 */
        InboxNotification: {
            /** Format: int64 */
            created_at: number;
            /** @description 页面、评论串、谁写的之类（按 type 不同） */
            data: Record<string, unknown>;
            /** Format: int64 */
            id: number;
            /**
             * Format: int64
             * @description 没读是 null
             */
            read_at?: number | null;
            /** @description mention / reply / resolved */
            type: string;
            workspace_id: string;
        };
        InviteBy: {
            name: string;
        };
        /** @description 邀请链接打开时看到的（不用登录）。 */
        InviteInfo: {
            /** Format: int64 */
            expires_at?: number | null;
            invited_by?: components["schemas"]["InviteBy"] | null;
            role: components["schemas"]["Role"];
            /** @description 能不能凭它注册 */
            signup: boolean;
            workspace: components["schemas"]["InviteWorkspace"];
        };
        InviteList: {
            /** @description 还能用的 */
            invites: components["schemas"]["InviteView"][];
        };
        /** @description 一条邀请链接。 */
        InviteView: {
            /** Format: int64 */
            created_at: number;
            /**
             * Format: int64
             * @description 毫秒时间戳；null = 不过期
             */
            expires_at?: number | null;
            /** @description 凭它加入后的角色 */
            role: components["schemas"]["Role"];
            token: string;
            /** @description 发给别人的完整链接 */
            url: string;
            /**
             * Format: int64
             * @description 用过几次
             */
            uses: number;
        };
        /** @description 邀请到哪个工作区。 */
        InviteWorkspace: {
            icon?: string | null;
            id: string;
            name: string;
        };
        LoginBody: {
            email: string;
            password: string;
        };
        /** @description 我是谁、在哪些工作区。 */
        Me: {
            user: components["schemas"]["MeUser"];
            workspaces: components["schemas"]["MeWorkspace"][];
        };
        /** @description 改个人资料：只改给了的字段；给 null 是清掉。 */
        MePatch: {
            /** @description 头像（emoji 或图片地址） */
            avatar?: string | null;
            /** @description zh / en；null = 跟浏览器 */
            locale?: string | null;
            name?: string | null;
            /** @description IANA 时区名；null = 跟服务器 */
            timezone?: string | null;
        };
        MeUser: {
            avatar?: string | null;
            email?: string | null;
            id: string;
            kind: components["schemas"]["PrincipalKind"];
            /** @description zh / en / null（跟浏览器） */
            locale?: string | null;
            name: string;
            timezone?: string | null;
        };
        MeWorkspace: {
            icon?: string | null;
            id: string;
            name: string;
            role: components["schemas"]["Role"];
        };
        Member: {
            role: components["schemas"]["Role"];
            user: components["schemas"]["MemberUser"];
        };
        /** @description 工作区的人（按加入先后）和 Agent。 */
        MemberList: {
            agents: components["schemas"]["AgentView"][];
            members: components["schemas"]["Member"][];
        };
        MemberRole: {
            role: components["schemas"]["Role"];
            user_id: string;
        };
        MemberUser: {
            avatar?: string | null;
            email?: string | null;
            id: string;
            name: string;
        };
        /** @description 建库（POST /workspaces/{ws}/databases 的正文）。 */
        NewDb: {
            /** @description 放在同级里这个页面后面 */
            after?: string | null;
            parent_id?: string | null;
            /**
             * @description 表结构 `{ props: [{ name, type, … }] }`，和 DbSchema 一样，但属性可以不给 key（自动分配），
             *     选项可以只写名字（`options: ["做", "完"]`）。不给只有标题
             */
            schema?: Record<string, unknown> | null;
            title?: string | null;
            /** @description 视图（和 DbView 一样，id 可以不给）；不给是一个表格视图 */
            views?: Record<string, unknown>[] | null;
        };
        NewInvite: {
            /**
             * Format: int64
             * @description 多少秒后过期；不给 = 不过期
             */
            expires_in?: number | null;
            /** @description viewer / editor / admin / owner */
            role: string;
        };
        NewSecret: {
            /** @description 新的 webhook 签名密钥 */
            webhook_secret: string;
        };
        NewThread: {
            anchor?: components["schemas"]["Anchor"] | null;
            /** @description Markdown；`@[名字](agent:<id>)` / `@[名字](user:<id>)` 或 `@名字` 叫人 */
            body: string;
            notify?: components["schemas"]["Notify"] | null;
        };
        NewToken: {
            /** @description 新 token（旧的马上失效） */
            token: string;
        };
        NoteAgent: {
            id: string;
            name: string;
        };
        NoteComment: {
            /** @description `{ kind, id, name }` */
            author: Record<string, unknown>;
            body: string;
            id: string;
        };
        NotePage: {
            id: string;
            title: string;
            url: string;
        };
        /** @description 评论时叫谁：false = 谁也不叫，`["agent:<id>", ...]` = 只叫这些（不给 / null = 按规则）。 */
        Notify: boolean | string[];
        Page: {
            /** Format: int64 */
            created_at: number;
            created_by?: string | null;
            /** Format: int64 */
            deleted_at?: number | null;
            icon?: string | null;
            id: string;
            /** @description page / database / row。 */
            kind?: string;
            parent_id?: string | null;
            position: string;
            title: string;
            /** Format: int64 */
            updated_at: number;
            workspace_id: string;
        };
        PageList: {
            /** @description 不含回收站里的、库里的行；按 (parent_id, position) 排 */
            pages: components["schemas"]["Page"][];
        };
        PasswordBody: {
            /** @description 现在的密码 */
            current: string;
            /** @description 新密码 */
            new: string;
        };
        PatchAgent: {
            about?: string | null;
            /** @description 通知里那段话用的语言：zh / en；null = 跟工作区 */
            locale?: string | null;
            name?: string | null;
            /** @description read / write / admin */
            scope?: string | null;
            /** @description null = 不用 webhook */
            webhook?: string | null;
        };
        PatchPage: {
            after?: string | null;
            icon?: string | null;
            parent_id?: string | null;
            title?: string | null;
        };
        PatchRows: {
            ops: components["schemas"]["CellOp"][];
        };
        PatchThread: {
            /** @description true = 解决，false = 重新打开 */
            resolved: boolean;
        };
        /** @description 改了的格子和改完的 seq。 */
        Patched: {
            /** @description 真的变了的格子（值没变的不在里面） */
            changes: components["schemas"]["CellChange"][];
            /** Format: int64 */
            seq: number;
        };
        /** @description 按按钮的结果。 */
        Pressed: {
            /** @description 按钮设的属性里真的变了的格子 */
            changes: components["schemas"]["CellChange"][];
            /** Format: int64 */
            seq: number;
            /** @description 按钮带评论时，发在这一行上的评论串 */
            thread_id?: string;
        };
        /** @enum {string} */
        PrincipalKind: "user" | "agent";
        /**
         * @description 属性类型。
         * @enum {string}
         */
        PropType: "title" | "text" | "number" | "select" | "multi_select" | "status" | "date" | "person" | "checkbox" | "url" | "email" | "phone" | "files" | "created_time" | "updated_time" | "created_by" | "updated_by" | "relation" | "rollup" | "formula" | "unique_id" | "button";
        PutContent: {
            /** @description 读的时候拿到的版本：在那个版本上改，这期间别人写的不会被覆盖 */
            base?: string | null;
            doc?: components["schemas"]["DocNode"] | null;
            markdown?: string | null;
        };
        /** @description 分享设置；不给正文就都是 false。 */
        PutShare: {
            /** @description 子页面也公开 */
            include_children?: boolean;
            /** @description 允许搜索引擎收录 */
            indexable?: boolean;
        };
        /** @description 换掉全部视图的正文：视图的数组，或 `{ views: [...] }`。 */
        PutViews: components["schemas"]["ViewObj"][] | {
            views: components["schemas"]["ViewObj"][];
        };
        /** @description 一个分组。空值的分组 `key` 是 ""，排在最后。 */
        QueryGroup: {
            color?: string;
            count: number;
            /** @description 子分组（视图有 sub_group_by 时） */
            groups?: components["schemas"]["QueryGroup"][];
            ids: string[];
            key: string;
            name: string;
            totals?: Record<string, unknown>;
        };
        /** @description 服务器算的查询（给 Agent、给不想加载全部数据的客户端）。给了 view_id 就从那个视图出发，别的字段覆盖它。 */
        QueryReq: {
            /** @description 筛选 `{ op: "and" | "or", items: [...] }` */
            filter?: Record<string, unknown> | null;
            group_by?: string | null;
            limit?: number | null;
            offset?: number | null;
            search?: string | null;
            /** @description 排序 `[{ key, dir }]` */
            sorts?: Record<string, unknown>[] | null;
            /** @description 每列底部的汇总：`{ 属性 key: 汇总方式 }` */
            totals?: Record<string, unknown> | null;
            view_id?: string | null;
        };
        /** @description 查询结果。 */
        QueryResult: {
            /** @description 按 group_by 分的组；没分组不给 */
            groups?: components["schemas"]["QueryGroup"][];
            /** @description 按排序的行 id（给了 limit / offset 是那一段） */
            ids: string[];
            /** @description 这一段的行，带算出来的值 */
            rows: components["schemas"]["QueryRow"][];
            /**
             * Format: int64
             * @description 库现在的 seq
             */
            seq: number;
            /** @description 符合条件的总行数 */
            total: number;
            /** @description 每列底部的汇总（键是属性的 key）；没要不给 */
            totals?: Record<string, unknown>;
        };
        /**
         * @description 查出来的一行：存的样子（`props`，键是属性的 key，空值不存），加上 `computed`（公式、汇总、反向关联算出来的值）。
         *     和 `DbRow` 不同：不带 `db_id`、`position`（顺序看 `ids`），时间是毫秒数。
         */
        QueryRow: {
            computed: Record<string, unknown>;
            /**
             * Format: double
             * @description 毫秒时间戳
             */
            created_at?: number;
            created_by?: string;
            id: string;
            props: Record<string, unknown>;
            /** Format: int64 */
            seq: number;
            /**
             * Format: int64
             * @description 自动编号
             */
            unique_id?: number;
            /**
             * Format: double
             * @description 毫秒时间戳
             */
            updated_at?: number;
            updated_by?: string;
        };
        ReadCount: {
            /**
             * Format: int64
             * @description 标了几条
             */
            read: number;
        };
        /** @description 标成已读：给 ids，或者 `all: true`。 */
        ReadNotes: {
            /** @description 全部 */
            all?: boolean;
            /** @description 这些编号 */
            ids?: number[] | null;
        };
        /** @description 关联：指向哪个库；`mirror_of` 有值时是双向关联的反向那一半。 */
        RelationConf: {
            db_id: string;
            mirror_of?: string;
        };
        Reply: {
            /** @description Markdown */
            body: string;
            notify?: components["schemas"]["Notify"] | null;
        };
        Restored: {
            /** @description 恢复后新存的一版（kind = restore） */
            version: components["schemas"]["VersionMeta"];
        };
        /** @description 放回来的那个页面，加上 `restored`（连同放回来的所有页面 id）。 */
        RestoredPage: components["schemas"]["Page"] & {
            /** @description 放回来的所有页面（这个和一起删的子页面） */
            restored: string[];
        };
        /** @enum {string} */
        Role: "viewer" | "editor" | "admin" | "owner";
        RoleBody: {
            /** @description viewer / editor / admin / owner */
            role: string;
        };
        /** @description 汇总：沿关联属性 `relation` 取对方库的属性 `prop`，按 `fn` 归总。 */
        RollupConf: {
            fn: string;
            prop: string;
            relation: string;
        };
        /** @description 增量（GET /databases/{id}/rows）。 */
        RowsDelta: {
            deleted: string[];
            rows: components["schemas"]["DbRow"][];
            /** Format: int64 */
            seq: number;
        };
        SaveBody: {
            /** @description 这一版的名字 */
            label?: string | null;
        };
        /** @description 改完的表结构。 */
        SchemaChanged: {
            /** @description 动了哪些属性（`{ op, key, name, … }`） */
            changes: Record<string, unknown>[];
            /** @description 新的表结构 */
            schema: components["schemas"]["DbSchema"];
            /**
             * Format: int64
             * @description 这次改动的 seq（历史里撤销用；什么也没变时是现在的 seq）
             */
            seq: number;
            /** @description 改了但有问题的地方（比如公式有错）；没有就不给 */
            warnings?: string[];
        };
        SchemaOps: {
            /** @description 依次做的改动：`{ op: "add" | "update" | "delete" | "move" | …, … }`，见 docs/phase4-contract.md */
            ops: Record<string, unknown>[];
        };
        /** @description 撤销后的表结构和视图。 */
        SchemaUndone: {
            schema: components["schemas"]["DbSchema"];
            /** Format: int64 */
            seq: number;
            views: components["schemas"]["DbView"][];
        };
        /** @enum {string} */
        Scope: "read" | "write" | "admin";
        /** @description 一条搜索结果（GET /workspaces/{ws}/search）。 */
        SearchResult: {
            block_id?: string | null;
            /** @description 这个页面命中了几处（标题、正文的段） */
            blocks: number;
            /** @description 行所在的库 */
            db_id?: string | null;
            icon?: string | null;
            /** @description page / database / row */
            kind: string;
            page_id: string;
            /** Format: float */
            score: number;
            snippet: string;
            title: string;
            /** Format: int64 */
            updated_at: number;
        };
        SearchResults: {
            results: components["schemas"]["SearchResult"][];
        };
        /** @description 选项。status 的有 `group`：todo / doing / done。 */
        SelectOption: {
            color: string;
            group?: string;
            id: string;
            name: string;
        };
        /** @description 改完后库的 seq。 */
        Seq: {
            /** Format: int64 */
            seq: number;
        };
        /** @description 页面的公开分享链接。 */
        ShareView: {
            /** Format: int64 */
            created_at: number;
            /** @description 子页面也公开 */
            include_children: boolean;
            /** @description 允许搜索引擎收录 */
            indexable: boolean;
            token: string;
            /** @description 公开的网址（`/s/<token>`） */
            url: string;
        };
        SignupBody: {
            email: string;
            /** @description 邀请的 token（OAD_SIGNUP=invite 时必须有），注册完直接加入那个工作区 */
            invite?: string | null;
            /** @description 显示的名字；不给就用邮箱 @ 前面那段 */
            name?: string;
            password: string;
        };
        /**
         * @description 注册方式。
         * @enum {string}
         */
        SignupMode: "invite" | "open" | "off";
        ThreadList: {
            threads: components["schemas"]["ThreadView"][];
        };
        ThreadView: {
            anchor?: components["schemas"]["Anchor"] | null;
            comments: components["schemas"]["CommentView"][];
            /** Format: int64 */
            created_at: number;
            created_by: components["schemas"]["AuthorRef"];
            id: string;
            page_id: string;
            /** Format: int64 */
            resolved_at?: number | null;
            resolved_by?: components["schemas"]["AuthorRef"] | null;
        };
        TrashItem: {
            /** Format: int64 */
            deleted_at: number;
            deleted_by?: components["schemas"]["AuthorRef"] | null;
            icon?: string | null;
            id: string;
            kind: string;
            parent_id?: string | null;
            /** @description 原来在哪个页面下（行是所在的库） */
            parent_title?: string | null;
            title: string;
        };
        /** @description 回收站里的页面、库、行（只列单独删的，新删的在前）。 */
        TrashList: {
            items: components["schemas"]["TrashItem"][];
            /** @description 还有下一页时给 */
            next_cursor?: string | null;
        };
        Undo: {
            /**
             * Format: int64
             * @description 要撤销的那次表结构改动
             */
            seq: number;
        };
        UndoBody: {
            /**
             * Format: int64
             * @description 要撤销的那次改动（历史里的 seq）
             */
            seq: number;
        };
        /**
         * @description 链接卡片。除了 kind / provider / source / url 都可能缺；按 kind 还有别的字段
         *     （canonical_url、description、site_name、favicon、author、thumbnail、media、published_at、embed；
         *     上传的文件有 name、mime、size、width、height、blurhash、duration_ms；本站的库有 db_id、view_id、icon、views）。
         */
        UnfurlCard: {
            /** @description video / post / repo / issue / pull_request / article / image / file / bookmark / database */
            kind: string;
            /** @description youtube / x / github / bilibili / wechat / generic；上传的文件是 upload，本站的库是 oad */
            provider: string;
            /** @description api / syndication / oembed / html / url_only */
            source: string;
            /** @description 过期了先给旧的，后台在刷新 */
            stale?: boolean | null;
            title?: string | null;
            url: string;
        };
        /** @description multipart 上传的表单。 */
        UploadForm: {
            /** @description 文件（文件名取这个字段的 filename） */
            file: string;
        };
        /** @description 上传接口的返回。 */
        Uploaded: {
            blurhash?: string | null;
            /** Format: int64 */
            duration_ms?: number | null;
            /** Format: int64 */
            height?: number | null;
            id: string;
            mime: string;
            name: string;
            /** Format: int64 */
            size: number;
            url: string;
            /** Format: int64 */
            width?: number | null;
        };
        /** @description 一版里改过的人。 */
        VersionAuthor: {
            id: string;
            kind: components["schemas"]["PrincipalKind"];
            name: string;
        };
        /** @description 某一版的内容。 */
        VersionContent: {
            doc: components["schemas"]["DocNode"];
            markdown: string;
            version: components["schemas"]["VersionMeta"];
        };
        VersionDiff: {
            /** @description 按块的增删改，按新的顺序 */
            blocks: components["schemas"]["BlockDiff"][];
            /** Format: int64 */
            from: number;
            to: components["schemas"]["DiffTo"];
        };
        /** @description 版本列表，新的在前。 */
        VersionList: {
            /** @description 还有下一页时给，带上它再取 */
            next_cursor?: string | null;
            versions: components["schemas"]["VersionMeta"][];
        };
        /** @description 历史版本（不含快照）。kind：auto（停下来 / 改得多了自动存）、agent（Agent 的一次写入）、restore（恢复）、manual（手动存，带名字）。 */
        VersionMeta: {
            authors: components["schemas"]["VersionAuthor"][];
            /** Format: int64 */
            created_at: number;
            created_by?: string | null;
            /** Format: int64 */
            id: number;
            kind: string;
            label?: string | null;
            page_id: string;
        };
        /** @description 一个视图（DbView 的样子；id 不给或重复时分配新的，type 不给是 table，name 不给是“视图”）。 */
        ViewObj: Record<string, unknown>;
        /**
         * @description 视图类型。
         * @enum {string}
         */
        ViewType: "table" | "board" | "calendar" | "gallery" | "list";
        /** @description 库的全部视图。 */
        Views: {
            views: components["schemas"]["DbView"][];
        };
        Workspace: {
            /** Format: int64 */
            created_at: number;
            icon?: string | null;
            id: string;
            name: string;
        };
        WsPatch: {
            /** @description 图标（emoji 或图片地址）；null = 去掉 */
            icon?: string | null;
            /** @description 新名字 */
            name?: string | null;
        };
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
    poll_notes: {
        parameters: {
            query?: {
                /** @description 从这个编号之后取 */
                after?: number;
                /** @description 同 after（上次的 next_cursor） */
                cursor?: string;
                /** @description 最多几条，默认 50，最多 100 */
                limit?: number;
                /** @description 没有就等几秒，默认 0，最多 60 */
                wait?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AgentNotes"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    ack_notes: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["Ack"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Acked"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    stream_notes: {
        parameters: {
            query?: {
                /** @description 从这个编号之后 */
                after?: number;
                /** @description 同 after */
                cursor?: string;
            };
            header?: {
                /** @description 断线重连时浏览器自动带的 */
                "Last-Event-ID"?: string | null;
            };
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 事件流，每条 data 是一个 AgentNotification */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/event-stream": components["schemas"]["AgentNotification"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    revoke_agent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Agent */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Agent"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    update_agent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Agent */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PatchAgent"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Agent"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 冲突 */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    rotate_token: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Agent */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["NewToken"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    rotate_secret: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Agent */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["NewSecret"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    auth_config: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AuthConfigBody"];
                };
            };
        };
    };
    login: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["LoginBody"];
            };
        };
        responses: {
            /** @description 登录了，Set-Cookie 带会话 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Me"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 请求太多 */
            429: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    logout: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功，没有内容 */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    signup: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SignupBody"];
            };
        };
        responses: {
            /** @description 注册了，Set-Cookie 带会话 */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Me"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 邮箱注册过了 */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 邀请过期了 */
            410: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 请求太多 */
            429: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    delete_comment: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 评论 */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功，没有内容 */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    edit_comment: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 评论 */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["EditComment"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CommentView"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    info: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DbInfo"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    press: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
                /** @description 行的 id */
                row: string;
                /** @description 按钮属性的 key 或名字 */
                key: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Pressed"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    export: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    /** @description attachment; filename*=UTF-8''<库名>.csv */
                    "Content-Disposition"?: string;
                    [name: string]: unknown;
                };
                content: {
                    "text/csv": string;
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    db_history: {
        parameters: {
            query?: {
                /** @description 只看这一行 */
                row?: string;
                /** @description 只看这个属性（key 或名字） */
                prop?: string;
                /** @description 一页几条，默认 200，最多 1000 */
                limit?: number;
                /** @description 上一页给的 `next_cursor` */
                cursor?: string;
            };
            header?: never;
            path: {
                /** @description 库的 id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DbHistory"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    db_undo: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 库的 id */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UndoBody"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DbUndone"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    query: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["QueryReq"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["QueryResult"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    rows: {
        parameters: {
            query?: {
                /** @description 只要这个 seq 之后改了的行（和删了的行 id）；不给是全量 */
                since?: number;
            };
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 行（全量或 since 之后的增量）。按 Accept-Encoding 压缩：br 优先，其次 gzip；不到 1KB 的不压 */
            200: {
                headers: {
                    /** @description br / gzip（压了才有） */
                    "Content-Encoding"?: string;
                    /** @description accept-encoding */
                    Vary?: string;
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["RowsDelta"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    add_rows: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["AddRows"];
            };
        };
        responses: {
            /** @description 建好了 */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AddedRows"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    delete_rows: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["Ids"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Seq"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    patch_rows: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PatchRows"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Patched"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    delete_one: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
                /** @description 行的 id */
                row: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Seq"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    change_schema: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SchemaOps"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SchemaChanged"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    undo_schema: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["Undo"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SchemaUndone"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    put_views: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PutViews"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Views"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    add_view: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": Record<string, unknown>;
            };
        };
        responses: {
            /** @description 加好的视图 */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DbView"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    delete_view: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
                /** @description 视图的 id */
                view: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功，没有内容 */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    patch_view: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
                /** @description 视图的 id */
                view: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": Record<string, unknown>;
            };
        };
        responses: {
            /** @description 改好的视图 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DbView"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_file_bare: {
        parameters: {
            query?: {
                /** @description 缩略图宽度（图片才有，给 WebP） */
                w?: number;
            };
            header?: {
                /** @description `bytes=a-b`，只认一段 */
                Range?: string | null;
            };
            path: {
                /** @description 文件 */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 文件内容 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/octet-stream": unknown;
                };
            };
            /** @description Range 的那一段 */
            206: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/octet-stream": unknown;
                };
            };
            /** @description If-None-Match 对上了 */
            304: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description Range 不对 */
            416: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    get_file: {
        parameters: {
            query?: {
                /** @description 缩略图宽度（图片才有，给 WebP） */
                w?: number;
            };
            header?: {
                /** @description `bytes=a-b`，只认一段 */
                Range?: string | null;
            };
            path: {
                /** @description 文件 */
                id: string;
                /** @description 文件名（只为好看，不校验） */
                name: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 文件内容 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/octet-stream": unknown;
                };
            };
            /** @description Range 的那一段 */
            206: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/octet-stream": unknown;
                };
            };
            /** @description If-None-Match 对上了 */
            304: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description Range 不对 */
            416: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    invite_info: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 邀请 */
                token: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["InviteInfo"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 过期或撤销了 */
            410: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    accept: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 邀请 */
                token: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Me"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 过期或撤销了 */
            410: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    me: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Me"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    update_me: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["MePatch"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Me"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    my_notes: {
        parameters: {
            query?: {
                /** @description 从这个编号往旧的翻 */
                after?: number;
                /** @description 同 after（上次的 next_cursor） */
                cursor?: string;
                /** @description 最多几条，默认 50，最多 100 */
                limit?: number;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Inbox"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    read_my_notes: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ReadNotes"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ReadCount"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    change_password: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PasswordBody"];
            };
        };
        responses: {
            /** @description 成功，没有内容 */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_media: {
        parameters: {
            query?: {
                /** @description 缩略图宽度 */
                w?: number;
            };
            header?: {
                /** @description `bytes=a-b`，只认一段 */
                Range?: string | null;
            };
            path: {
                /** @description 内容的 hash */
                hash: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 图片 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/octet-stream": unknown;
                };
            };
            /** @description Range 的那一段 */
            206: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/octet-stream": unknown;
                };
            };
            /** @description If-None-Match 对上了 */
            304: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description Range 不对 */
            416: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    get_page: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Page"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    delete_page: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Deleted"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    patch_page: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PatchPage"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Page"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    authors: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 页面 */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Authors"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    insert_blocks: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["BlockBody"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["BlockIds"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_block: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
                block_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["BlockContent"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    put_block: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
                block_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["BlockBody"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["BlockIds"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    delete_block: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
                block_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功，没有内容 */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_comments: {
        parameters: {
            query?: {
                /** @description false（默认，没解决的）/ true / all */
                resolved?: string;
            };
            header?: never;
            path: {
                /** @description 页面 */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ThreadList"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    create_thread: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 页面 */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["NewThread"];
            };
        };
        responses: {
            /** @description 建好了 */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ThreadView"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_content: {
        parameters: {
            query?: {
                /** @description markdown（默认）/ json */
                format?: string;
                /** @description 只要前 N 个顶层块（先画第一屏），响应里带 `total` */
                limit?: number;
            };
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Content"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    put_content: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PutContent"];
            };
        };
        responses: {
            /** @description 成功，没有内容 */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    get_share: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 页面 */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ShareView"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 页面不存在或没分享 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    put_share: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 页面 */
                id: string;
            };
            cookie?: never;
        };
        /** @description 可以不给 */
        requestBody?: {
            content: {
                "application/json": components["schemas"]["PutShare"] | null;
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ShareView"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    del_share: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 页面 */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功，没有内容 */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    history_list: {
        parameters: {
            query?: {
                /** @description 一页几条，默认 200 */
                limit?: number;
                /** @description 上一页给的 `next_cursor` */
                cursor?: string;
            };
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["VersionList"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    save: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
            };
            cookie?: never;
        };
        /** @description 可以不带正文 */
        requestBody?: {
            content: {
                "application/json": components["schemas"]["SaveBody"] | null;
            };
        };
        responses: {
            /** @description 手动存的一版（kind = manual） */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["VersionMeta"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    one: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
                /** @description 版本号 */
                v: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["VersionContent"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    diff: {
        parameters: {
            query?: {
                /** @description 和哪一版比：版本号，或 `current`（默认，现在的正文） */
                to?: string;
            };
            header?: never;
            path: {
                id: string;
                /** @description 版本号 */
                v: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["VersionDiff"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    history_restore: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                id: string;
                /** @description 版本号 */
                v: number;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Restored"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    handler: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                page_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 升级成 WebSocket */
            101: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    patch_thread: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 评论串 */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PatchThread"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ThreadView"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    reply: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 评论串 */
                id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["Reply"];
            };
        };
        responses: {
            /** @description 建好了 */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CommentView"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    purge: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 回收站里的页面 id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 彻底删了（连子页面、正文、版本、评论） */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    trash_restore: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 回收站里的页面 id */
                id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["RestoredPage"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    unfurl: {
        parameters: {
            query: {
                /** @description 要解析的网址 */
                url: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["UnfurlCard"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 外部网站出错 */
            502: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    update_ws: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["WsPatch"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Workspace"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_agents: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AgentList"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    create_agent: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateAgent"];
            };
        };
        responses: {
            /** @description 建好了 */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreatedAgent"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 冲突 */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    create: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["NewDb"];
            };
        };
        responses: {
            /** @description 建好了 */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DbInfo"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    import: {
        parameters: {
            query?: {
                /** @description 库名（multipart 里的 title 优先；都没有用文件名） */
                title?: string;
                /** @description 放在哪个页面下（不给放在根上） */
                parent_id?: string;
            };
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
            };
            cookie?: never;
        };
        /** @description multipart 的 file 字段，或者原始的 CSV 正文；最大 64MB */
        requestBody: {
            content: {
                "multipart/form-data": components["schemas"]["ImportForm"];
                "text/csv": string;
            };
        };
        responses: {
            /** @description 建好了 */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["DbInfo"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 太大 */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    events: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                ws: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 事件流 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/event-stream": unknown;
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    upload: {
        parameters: {
            query?: never;
            header?: {
                /** @description 原始正文上传时的文件名（百分号编码），默认 file */
                "X-Filename"?: string | null;
            };
            path: {
                /** @description 工作区 */
                ws: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/octet-stream": unknown;
                "multipart/form-data": components["schemas"]["UploadForm"];
            };
        };
        responses: {
            /** @description 建好了 */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Uploaded"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 太大 */
            413: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_invites: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["InviteList"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    create_invite: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["NewInvite"];
            };
        };
        responses: {
            /** @description 建好了 */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["InviteView"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    revoke_invite: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
                /** @description 邀请 */
                token: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功，没有内容 */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    members: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["MemberList"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    remove_member: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
                /** @description 用户 */
                user: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功，没有内容 */
            204: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description 会去掉最后一个所有者 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    set_role: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
                /** @description 用户 */
                user: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["RoleBody"];
            };
        };
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["MemberRole"];
                };
            };
            /** @description 角色不对，或者会去掉最后一个所有者 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    list_pages: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PageList"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    create_page: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                ws: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreatePage"];
            };
        };
        responses: {
            /** @description 建好了 */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["Page"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    search: {
        parameters: {
            query: {
                /** @description 搜索词 */
                q: string;
                /** @description 最多几条，默认 20，最多 100 */
                limit?: number;
                /** @description page / row / all（默认） */
                kind?: string;
                /** @description `block`：每块一条（同一页最多两条）；默认每页一条 */
                per?: string;
            };
            header?: never;
            path: {
                ws: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SearchResults"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    trash_list: {
        parameters: {
            query?: {
                /** @description 一页几条，默认 200 */
                limit?: number;
                /** @description 上一页给的 `next_cursor` */
                cursor?: string;
            };
            header?: never;
            path: {
                /** @description 工作区 */
                ws: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description 成功 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["TrashList"];
                };
            };
            /** @description 参数不对 */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有登录 */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 没有权限 */
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
            /** @description 找不到 */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorBody"];
                };
            };
        };
    };
    agent_notification: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": components["schemas"]["AgentNotification"];
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    comment_created: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": {
                    comment_id?: string;
                    page_id?: string;
                    thread?: components["schemas"]["ThreadView"];
                    thread_id?: string;
                };
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    comment_deleted: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": Record<string, unknown>;
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    comment_updated: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": Record<string, unknown>;
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    db_changed: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": {
                    db_id?: string;
                    seq?: number;
                };
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    db_schema: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": {
                    db_id?: string;
                };
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    notification: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": Record<string, unknown>;
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    page_created: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": components["schemas"]["Page"];
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    page_deleted: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": components["schemas"]["Page"];
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    page_moved: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": components["schemas"]["Page"];
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    page_purged: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": {
                    id?: string;
                };
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    page_updated: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": components["schemas"]["Page"];
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    resync: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": Record<string, unknown>;
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    thread_updated: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": Record<string, unknown>;
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    workspace_updated: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: {
            content: {
                "application/json": components["schemas"]["Workspace"];
            };
        };
        responses: {
            /** @description 收到 */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
}
