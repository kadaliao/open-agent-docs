package oad

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// 服务器回的错误按状态码对应这些哨兵，用 errors.Is 判断：
//
//	if errors.Is(err, oad.ErrNotFound) { … }
//
// 要状态码、错误码、信息就 errors.As 成 *APIError。
var (
	ErrBadRequest   = errors.New("oad: 400 参数不对")
	ErrUnauthorized = errors.New("oad: 401 没有登录")
	ErrForbidden    = errors.New("oad: 403 没有权限")
	ErrNotFound     = errors.New("oad: 404 找不到")
	ErrConflict     = errors.New("oad: 409 冲突")
	ErrGone         = errors.New("oad: 410 已经没了")
	ErrTooLarge     = errors.New("oad: 413 太大")
	ErrRateLimited  = errors.New("oad: 429 请求太多")
	ErrServer       = errors.New("oad: 服务器出错（5xx）")
)

// 连接、配置方面的错误。
var (
	// ErrConnection：连不上服务器、连接断了（*ConnectionError）。
	ErrConnection = errors.New("oad: 连不上服务器")
	// ErrTimeout：请求超时（WithTimeout）。这时 errors.Is(err, context.DeadlineExceeded) 也是 true。
	ErrTimeout = errors.New("oad: 请求超时")
	// ErrConfig：客户端配置不对（比如 WithUser 时既没有 header secret 也没有 JWT secret）。
	ErrConfig = errors.New("oad: 配置不对")
)

// APIError 是服务器回的错误（状态码 ≥ 400）。
type APIError struct {
	// StatusCode 是 HTTP 状态码。
	StatusCode int
	// Code 取响应的 error.code（not_found、bad_request……）；响应不是 JSON 时是 http_<状态码>。
	Code string
	// Message 是给人看的信息（语言看 WithLocale）。
	Message string
	// RetryAfter 是响应头 Retry-After（429 时有），没有是 0。
	RetryAfter time.Duration

	method, path string
}

func (e *APIError) Error() string {
	msg := e.Message
	if msg == "" {
		msg = http.StatusText(e.StatusCode)
	}
	return fmt.Sprintf("oad: %s %s: %d %s: %s", e.method, e.path, e.StatusCode, e.Code, msg)
}

// Is 让 errors.Is(err, oad.ErrNotFound) 之类按状态码判断。
func (e *APIError) Is(target error) bool {
	switch target {
	case ErrBadRequest:
		return e.StatusCode == 400
	case ErrUnauthorized:
		return e.StatusCode == 401
	case ErrForbidden:
		return e.StatusCode == 403
	case ErrNotFound:
		return e.StatusCode == 404
	case ErrConflict:
		return e.StatusCode == 409
	case ErrGone:
		return e.StatusCode == 410
	case ErrTooLarge:
		return e.StatusCode == 413
	case ErrRateLimited:
		return e.StatusCode == 429
	case ErrServer:
		return e.StatusCode >= 500
	}
	return false
}

func newAPIError(method, path string, resp *http.Response, body []byte) *APIError {
	e := &APIError{StatusCode: resp.StatusCode, Code: "http_" + strconv.Itoa(resp.StatusCode), method: method, path: path}
	var eb struct {
		Error *struct {
			Code    string `json:"code"`
			Message string `json:"message"`
		} `json:"error"`
	}
	if json.Unmarshal(body, &eb) == nil && eb.Error != nil {
		if eb.Error.Code != "" {
			e.Code = eb.Error.Code
		}
		e.Message = eb.Error.Message
	} else {
		e.Message = strings.TrimSpace(string(body))
		if len(e.Message) > 500 {
			e.Message = e.Message[:500] + "…"
		}
		if e.Message == "" {
			e.Message = http.StatusText(resp.StatusCode)
		}
	}
	e.RetryAfter = parseRetryAfter(resp.Header.Get("Retry-After"))
	return e
}

// Retry-After：秒数，或者 HTTP 日期。
func parseRetryAfter(v string) time.Duration {
	v = strings.TrimSpace(v)
	if v == "" {
		return 0
	}
	if n, err := strconv.ParseFloat(v, 64); err == nil && n >= 0 {
		return time.Duration(n * float64(time.Second))
	}
	if t, err := http.ParseTime(v); err == nil {
		if d := time.Until(t); d > 0 {
			return d
		}
	}
	return 0
}

// ConnectionError：请求没拿到响应（连不上、连接断了、超时）。Err 是原来的错误。
type ConnectionError struct {
	Method, URL string
	// Timeout：是 WithTimeout 的超时（或 HTTP 客户端自己的超时）。
	Timeout bool
	Err     error
}

func (e *ConnectionError) Error() string {
	if e.Timeout {
		return fmt.Sprintf("oad: %s %s: 请求超时: %v", e.Method, e.URL, e.Err)
	}
	return fmt.Sprintf("oad: %s %s: 连不上服务器: %v", e.Method, e.URL, e.Err)
}

func (e *ConnectionError) Unwrap() error { return e.Err }

// Is：所有连接错误都是 ErrConnection；超时还是 ErrTimeout 和 context.DeadlineExceeded。
func (e *ConnectionError) Is(target error) bool {
	switch target {
	case ErrConnection:
		return true
	case ErrTimeout, context.DeadlineExceeded:
		return e.Timeout
	}
	return false
}

func configError(format string, a ...any) error {
	return fmt.Errorf("%w：%s", ErrConfig, fmt.Sprintf(format, a...))
}
