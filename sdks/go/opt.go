package oad

import (
	"bytes"
	"encoding/json"
	"fmt"
)

// Opt 是可选字段：没设置、null、有值三种状态。
//
// 请求里：零值 Opt{} 是“没设置”，这个字段不发；oad.Some(v) 发 v；oad.Null[T]() 明确发 null
// （比如 pages.update 的 After: oad.Null[string]() = 挪到最前）。
//
// 响应里：服务器没给这个字段是没设置，给了 null 是 IsNull，给了值用 Get / Or / Value 取。
// 全包统一：模型里不是必填的字段、可以是 null 的字段都是 Opt；必填的是普通类型。
type Opt[T any] struct {
	value T
	state optState
}

type optState uint8

const (
	optUnset optState = iota
	optNull
	optValue
)

// Some 是有值的 Opt。
func Some[T any](v T) Opt[T] { return Opt[T]{value: v, state: optValue} }

// Null 是明确的 null（请求里会发 "字段": null）。
func Null[T any]() Opt[T] { return Opt[T]{state: optNull} }

// Get 返回值和“有没有值”（没设置、null 都是 false）。
func (o Opt[T]) Get() (T, bool) { return o.value, o.state == optValue }

// Value 返回值；没设置、null 时是零值。
func (o Opt[T]) Value() T { return o.value }

// Or 有值返回值，否则返回 def。
func (o Opt[T]) Or(def T) T {
	if o.state == optValue {
		return o.value
	}
	return def
}

// Valid 是不是有值（不是没设置，也不是 null）。
func (o Opt[T]) Valid() bool { return o.state == optValue }

// IsNull 是不是明确的 null。
func (o Opt[T]) IsNull() bool { return o.state == optNull }

// IsSet 设置了没有（有值或者 null 都算）。请求里没设置的字段不发。
func (o Opt[T]) IsSet() bool { return o.state != optUnset }

// IsZero 等于 !IsSet()：Go 1.24 以后的 `json:",omitzero"` 也会把没设置的去掉。
func (o Opt[T]) IsZero() bool { return o.state == optUnset }

// Ptr 有值时返回指向值的指针，否则 nil。
func (o Opt[T]) Ptr() *T {
	if o.state != optValue {
		return nil
	}
	v := o.value
	return &v
}

// String 方便打印：没设置是 <unset>，null 是 null。
func (o Opt[T]) String() string {
	switch o.state {
	case optUnset:
		return "<unset>"
	case optNull:
		return "null"
	}
	return fmt.Sprint(o.value)
}

// MarshalJSON：有值发值，其它发 null（没设置的字段由外面的结构体负责不写）。
func (o Opt[T]) MarshalJSON() ([]byte, error) {
	if o.state != optValue {
		return []byte("null"), nil
	}
	return json.Marshal(o.value)
}

// UnmarshalJSON：null → IsNull，别的 → 有值。字段不在 JSON 里时不会调，保持没设置。
func (o *Opt[T]) UnmarshalJSON(b []byte) error {
	if bytes.Equal(bytes.TrimSpace(b), []byte("null")) {
		var zero T
		o.value, o.state = zero, optNull
		return nil
	}
	if err := json.Unmarshal(b, &o.value); err != nil {
		return err
	}
	o.state = optValue
	return nil
}

func nonNilSlice[T any](s []T) []T {
	if s == nil {
		return []T{}
	}
	return s
}

func nonNilMap[K comparable, V any](m map[K]V) map[K]V {
	if m == nil {
		return map[K]V{}
	}
	return m
}
