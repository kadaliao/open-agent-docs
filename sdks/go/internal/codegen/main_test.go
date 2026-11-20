package main

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
)

// 生成结果是最新的：改了 openapi.json / operations.json / 生成器以后要在 sdks/go 下 go generate 再提交。
func TestGeneratedUpToDate(t *testing.T) {
	files, err := generateFiles("../../../../docs/openapi.json", "../../../operations.json")
	if err != nil {
		t.Fatal(err)
	}
	for name, want := range files {
		got, err := os.ReadFile(filepath.Join("../..", name))
		if err != nil {
			t.Errorf("%s 没生成：%v（在 sdks/go 下跑 go generate）", name, err)
			continue
		}
		if !bytes.Equal(got, want) {
			t.Errorf("%s 不是最新的：在 sdks/go 下跑 go generate", name)
		}
	}
	old, _ := filepath.Glob("../../zz_generated_*.go")
	for _, f := range old {
		if _, ok := files[filepath.Base(f)]; !ok {
			t.Errorf("%s 是多出来的旧文件：在 sdks/go 下跑 go generate", f)
		}
	}
}

// 同样的输入生成一样的东西（map 的顺序不影响输出）。
func TestGenerateDeterministic(t *testing.T) {
	a, err := generateFiles("../../../../docs/openapi.json", "../../../operations.json")
	if err != nil {
		t.Fatal(err)
	}
	for range 3 {
		b, err := generateFiles("../../../../docs/openapi.json", "../../../operations.json")
		if err != nil {
			t.Fatal(err)
		}
		for k := range a {
			if !bytes.Equal(a[k], b[k]) {
				t.Fatalf("%s 两次生成不一样", k)
			}
		}
	}
}

// 没登记名字的接口：报错，不悄悄漏掉。
func TestUnknownOperation(t *testing.T) {
	spec := []byte(`{"info":{"version":"1"},"paths":{"/x":{"get":{"operationId":"x","responses":{"204":{"description":"ok"}}}}},"components":{"schemas":{}}}`)
	if _, err := generate(spec, []byte(`{"operations":{},"skip":{}}`)); err == nil {
		t.Fatal("应该报错")
	}
	out, err := generate(spec, []byte(`{"operations":{"x":{"name":"things.ping"}},"skip":{}}`))
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Contains(out["zz_generated_resources.go"], []byte("func (r *ThingsService) Ping(ctx context.Context) error")) {
		t.Fatalf("生成的不对：\n%s", out["zz_generated_resources.go"])
	}
}
