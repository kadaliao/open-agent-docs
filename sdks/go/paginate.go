package oad

import (
	"context"
	"fmt"
	"iter"
	"net/url"
)

// paginate 按 next_cursor 一页一页往下取，逐条给出来（生成的 XxxAll 用）。
// 停止条件和 JS 的 paginate 一样：next_cursor 是空、这页没有条目、或者游标没变。
func paginate[T any](start string, fetch func(cursor string) ([]T, string, error)) iter.Seq2[T, error] {
	return func(yield func(T, error) bool) {
		cursor := start
		for {
			items, next, err := fetch(cursor)
			if err != nil {
				var zero T
				yield(zero, err)
				return
			}
			for _, x := range items {
				if !yield(x, nil) {
					return
				}
			}
			if next == "" || len(items) == 0 || next == cursor {
				return
			}
			cursor = next
		}
	}
}

// afterPagesCreate：Pages.Create 给了 Markdown 时再写正文。
func (c *core) afterPagesCreate(ctx context.Context, page *Page, params PagesCreateParams) (*Page, error) {
	if params.Markdown == "" {
		return page, nil
	}
	req := &request{method: "PUT", path: "/pages/" + url.PathEscape(page.ID) + "/content"}
	if err := req.setJSON(PutContent{Markdown: Some(params.Markdown)}); err != nil {
		return page, err
	}
	if err := c.do(ctx, req, nil); err != nil {
		return page, fmt.Errorf("oad: 页面 %s 建好了，但写正文失败: %w", page.ID, err)
	}
	return page, nil
}
