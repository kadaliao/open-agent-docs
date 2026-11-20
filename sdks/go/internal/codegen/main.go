// 生成器：读 docs/openapi.json 和 sdks/operations.json，生成 sdks/go 下的 zz_generated_*.go（模型、资源方法、事件表、版本号）。
//
// 在 sdks/go 下跑：go generate（或 go run ./internal/codegen）。能反复跑：openapi.json 改了重新跑一次就行，
// 输出只看输入（字段、接口按名字排序），同样的输入每次生成一样的文件；以前生成、现在不要的 zz_generated_*.go 会删掉。
package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"go/ast"
	"go/format"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

func main() {
	spec := flag.String("spec", "../../docs/openapi.json", "openapi.json 的路径")
	ops := flag.String("ops", "../operations.json", "operations.json 的路径")
	out := flag.String("out", ".", "输出目录（sdks/go）")
	flag.Parse()
	if err := run(*spec, *ops, *out); err != nil {
		fmt.Fprintln(os.Stderr, "codegen:", err)
		os.Exit(1)
	}
}

func run(specPath, opsPath, outDir string) error {
	files, err := generateFiles(specPath, opsPath)
	if err != nil {
		return err
	}
	// 先删掉旧的生成文件（换了文件名的不留下）
	old, _ := filepath.Glob(filepath.Join(outDir, "zz_generated_*.go"))
	for _, f := range old {
		if _, keep := files[filepath.Base(f)]; !keep {
			if err := os.Remove(f); err != nil {
				return err
			}
		}
	}
	for name, src := range files {
		p := filepath.Join(outDir, name)
		if cur, err := os.ReadFile(p); err == nil && bytes.Equal(cur, src) {
			continue
		}
		if err := os.WriteFile(p, src, 0o644); err != nil {
			return err
		}
	}
	return nil
}

// generateFiles 读两个输入，返回 文件名 → 内容（已经 gofmt）。
func generateFiles(specPath, opsPath string) (map[string][]byte, error) {
	specRaw, err := os.ReadFile(specPath)
	if err != nil {
		return nil, err
	}
	opsRaw, err := os.ReadFile(opsPath)
	if err != nil {
		return nil, err
	}
	return generate(specRaw, opsRaw)
}

// ---------- 输入 ----------

type Schema struct {
	Ref                  string             `json:"$ref"`
	Type                 json.RawMessage    `json:"type"`
	Format               string             `json:"format"`
	Description          string             `json:"description"`
	Properties           map[string]*Schema `json:"properties"`
	Required             []string           `json:"required"`
	Items                *Schema            `json:"items"`
	OneOf                []*Schema          `json:"oneOf"`
	AnyOf                []*Schema          `json:"anyOf"`
	AllOf                []*Schema          `json:"allOf"`
	Enum                 []any              `json:"enum"`
	AdditionalProperties json.RawMessage    `json:"additionalProperties"`
}

// types 是 type 字段：可能是字符串，也可能是数组（["string", "null"]）。
func (s *Schema) types() []string {
	if len(s.Type) == 0 {
		return nil
	}
	var one string
	if json.Unmarshal(s.Type, &one) == nil {
		return []string{one}
	}
	var many []string
	_ = json.Unmarshal(s.Type, &many)
	return many
}

type Media struct {
	Schema *Schema `json:"schema"`
}

type Parameter struct {
	Name        string  `json:"name"`
	In          string  `json:"in"`
	Required    bool    `json:"required"`
	Description string  `json:"description"`
	Schema      *Schema `json:"schema"`
}

type Operation struct {
	OperationID string       `json:"operationId"`
	Summary     string       `json:"summary"`
	Description string       `json:"description"`
	Parameters  []*Parameter `json:"parameters"`
	RequestBody *struct {
		Content map[string]Media `json:"content"`
	} `json:"requestBody"`
	Responses map[string]*struct {
		Description string           `json:"description"`
		Content     map[string]Media `json:"content"`
	} `json:"responses"`
}

type Spec struct {
	Info struct {
		Version string `json:"version"`
	} `json:"info"`
	Paths      map[string]map[string]json.RawMessage `json:"paths"`
	Components struct {
		Schemas map[string]*Schema `json:"schemas"`
	} `json:"components"`
	Webhooks map[string]map[string]*Operation `json:"webhooks"`
}

type Hint struct {
	Name      string `json:"name"`
	Unwrap    string `json:"unwrap"`
	Paginate  string `json:"paginate"`
	NullOn404 bool   `json:"null_on_404"`
	SSE       string `json:"sse"`
	Upload    string `json:"upload"`
	Download  bool   `json:"download"`
	JS        string `json:"js"`
}

type OpsTable struct {
	Operations map[string]Hint   `json:"operations"`
	Skip       map[string]string `json:"skip"`
}

// 手写的补充：生成的参数多几个字段，调完再走一个手写的函数。
type extraField struct{ name, typ, doc string }
type hook struct {
	fields []extraField
	after  string // core 上的方法：func (c *core) after(ctx, out, params) (out, error)
}

var hooks = map[string]hook{
	"pages.create": {
		fields: []extraField{{"Markdown", "string", "建好以后把这段 Markdown 写成正文（再发一个 PUT /pages/{id}/content）。空字符串就不写。"}},
		after:  "afterPagesCreate",
	},
}

// ---------- 名字 ----------

var initialisms = map[string]string{
	"id": "ID", "ids": "IDs", "url": "URL", "urls": "URLs", "csv": "CSV", "html": "HTML", "ws": "WS",
	"api": "API", "json": "JSON", "http": "HTTP", "jwt": "JWT", "sse": "SSE", "uri": "URI", "ip": "IP",
}

func word(w string) string {
	if w == "" {
		return ""
	}
	if s, ok := initialisms[strings.ToLower(w)]; ok {
		return s
	}
	return strings.ToUpper(w[:1]) + w[1:]
}

// camel：snake_case / 点路径 → 导出的 Go 名字（block_id → BlockID）。
func camel(s string) string {
	var b strings.Builder
	for _, w := range strings.FieldsFunc(s, func(r rune) bool { return r == '_' || r == '.' || r == '-' }) {
		b.WriteString(word(w))
	}
	return b.String()
}

// lowerCamel：参数名（block_id → blockID，id → id）。
func lowerCamel(s string) string {
	ws := strings.FieldsFunc(s, func(r rune) bool { return r == '_' || r == '-' })
	var b strings.Builder
	for i, w := range ws {
		if i == 0 {
			b.WriteString(strings.ToLower(w))
		} else {
			b.WriteString(word(w))
		}
	}
	out := b.String()
	switch out {
	case "ctx", "params", "req", "out", "err", "q", "type", "func", "range", "map", "chan", "go", "select", "default", "var", "case", "string", "len":
		return out + "Arg"
	}
	return out
}

// typeName：schema 的名字（CamelCase）按 Go 的缩写习惯改（BlockIds → BlockIDs，WsPatch → WSPatch）。
func typeName(s string) string {
	var words []string
	start := 0
	for i := 1; i < len(s); i++ {
		if s[i] >= 'A' && s[i] <= 'Z' {
			words = append(words, s[start:i])
			start = i
		}
	}
	words = append(words, s[start:])
	var b strings.Builder
	for _, w := range words {
		if v, ok := initialisms[strings.ToLower(w)]; ok {
			b.WriteString(v)
		} else {
			b.WriteString(w)
		}
	}
	return b.String()
}

func refName(ref string) string { return ref[strings.LastIndex(ref, "/")+1:] }

// ---------- 生成 ----------

type gen struct {
	spec    *Spec
	schemas map[string]*Schema
	// 生成的类型：Go 名字 → 代码；inline 的对象也在这里
	decls map[string]string
	// 正在生成的（防递归）
	pending map[string]bool
}

func generate(specRaw, opsRaw []byte) (map[string][]byte, error) {
	var spec Spec
	if err := json.Unmarshal(specRaw, &spec); err != nil {
		return nil, fmt.Errorf("openapi.json: %w", err)
	}
	var table OpsTable
	if err := json.Unmarshal(opsRaw, &table); err != nil {
		return nil, fmt.Errorf("operations.json: %w", err)
	}
	g := &gen{spec: &spec, schemas: spec.Components.Schemas, decls: map[string]string{}, pending: map[string]bool{}}

	// 模型
	names := sortedKeys(g.schemas)
	for _, n := range names {
		if err := g.named(n); err != nil {
			return nil, err
		}
	}

	ops, err := g.operations(&table)
	if err != nil {
		return nil, err
	}
	resources, err := g.resources(ops)
	if err != nil {
		return nil, err
	}
	events, err := g.events()
	if err != nil {
		return nil, err
	}

	// 模型最后写（资源、事件里可能又登记了 inline 的类型）
	var body bytes.Buffer
	for _, n := range sortedKeys(g.decls) {
		body.WriteString(g.decls[n])
		body.WriteString("\n")
	}
	var models bytes.Buffer
	header(&models, usedImports(body.Bytes(), "encoding/json"))
	models.Write(body.Bytes())

	var version bytes.Buffer
	header(&version, nil)
	fmt.Fprintf(&version, "// Version 是这个 SDK 的版本，和服务器同一个版本号（docs/openapi.json 的 info.version）。\n// Start 默认下载这个版本的服务器；请求头 User-Agent 也带它。\nconst Version = %q\n", spec.Info.Version)

	out := map[string][]byte{}
	for name, src := range map[string][]byte{
		"zz_generated_models.go":    models.Bytes(),
		"zz_generated_resources.go": resources,
		"zz_generated_events.go":    events,
		"zz_generated_version.go":   version.Bytes(),
	} {
		f, err := format.Source(src)
		if err != nil {
			return nil, fmt.Errorf("%s 格式化失败：%w\n%s", name, err, numbered(src))
		}
		out[name] = f
	}
	return out, nil
}

func numbered(src []byte) string {
	var b strings.Builder
	for i, l := range strings.Split(string(src), "\n") {
		fmt.Fprintf(&b, "%4d %s\n", i+1, l)
	}
	return b.String()
}

func header(b *bytes.Buffer, imports []string) {
	b.WriteString("// Code generated by internal/codegen from docs/openapi.json and sdks/operations.json. DO NOT EDIT.\n// 生成的，不要手改：改 openapi.json / operations.json 以后在 sdks/go 下跑 go generate。\n\npackage oad\n\n")
	if len(imports) > 0 {
		b.WriteString("import (\n")
		for _, i := range imports {
			fmt.Fprintf(b, "\t%q\n", i)
		}
		b.WriteString(")\n\n")
	}
}

func comment(b *bytes.Buffer, indent, text string) {
	text = strings.TrimSpace(text)
	if text == "" {
		return
	}
	for _, l := range strings.Split(text, "\n") {
		l = strings.TrimRight(l, " \t")
		if l == "" {
			fmt.Fprintf(b, "%s//\n", indent)
		} else {
			fmt.Fprintf(b, "%s// %s\n", indent, l)
		}
	}
}

func sortedKeys[V any](m map[string]V) []string {
	ks := make([]string, 0, len(m))
	for k := range m {
		ks = append(ks, k)
	}
	sort.Strings(ks)
	return ks
}

func (g *gen) resolve(s *Schema) *Schema {
	for s != nil && s.Ref != "" {
		s = g.schemas[refName(s.Ref)]
	}
	return s
}

// named 生成 components.schemas 里的一个类型。
func (g *gen) named(name string) error {
	goName := typeName(name)
	if _, done := g.decls[goName]; done || g.pending[goName] {
		return nil
	}
	g.pending[goName] = true
	defer delete(g.pending, goName)
	s := g.schemas[name]
	if s == nil {
		return fmt.Errorf("没有 schema %s", name)
	}
	var b bytes.Buffer
	ts := s.types()
	switch {
	case len(s.Enum) > 0 && len(ts) == 1 && ts[0] == "string":
		desc := s.Description
		if desc == "" {
			desc = "取值见下面的常量。"
		}
		comment(&b, "", goName+" "+desc)
		fmt.Fprintf(&b, "type %s string\n\n", goName)
		fmt.Fprintf(&b, "// %s 的取值。\nconst (\n", goName)
		for _, e := range s.Enum {
			v := fmt.Sprint(e)
			fmt.Fprintf(&b, "\t%s%s %s = %q\n", goName, camel(v), goName, v)
		}
		b.WriteString(")\n")
	case len(s.AllOf) > 0:
		merged := &Schema{Properties: map[string]*Schema{}, Description: s.Description}
		for _, part := range s.AllOf {
			p := g.resolve(part)
			for k, v := range p.Properties {
				merged.Properties[k] = v
			}
			merged.Required = append(merged.Required, p.Required...)
		}
		src, err := g.structDecl(goName, merged, merged.Description)
		if err != nil {
			return err
		}
		b.WriteString(src)
	case len(s.Properties) > 0:
		src, err := g.structDecl(goName, s, s.Description)
		if err != nil {
			return err
		}
		b.WriteString(src)
	default:
		t, _, err := g.goType(s, goName)
		if err != nil {
			return fmt.Errorf("%s: %w", name, err)
		}
		desc := s.Description
		if len(s.OneOf) > 0 {
			var vs []string
			for _, o := range s.OneOf {
				vt, _, err := g.goType(o, goName)
				if err != nil {
					return err
				}
				vs = append(vs, vt)
			}
			desc = strings.TrimSpace(desc + "\n\n几种形状之一（按 JSON 解出来是其中一种）：" + strings.Join(vs, "、") + "。")
		}
		if desc == "" {
			desc = "任意 JSON。"
		}
		comment(&b, "", goName+" "+desc)
		fmt.Fprintf(&b, "type %s = %s\n", goName, t)
	}
	g.decls[goName] = b.String()
	return nil
}

// field 是生成的结构体字段。
type field struct {
	json, goName, typ, doc string
	required, opt          bool
	kind                   string // slice / map / ""
}

func (g *gen) fields(owner string, s *Schema) ([]field, error) {
	req := map[string]bool{}
	for _, r := range s.Required {
		req[r] = true
	}
	var out []field
	seen := map[string]string{}
	for _, k := range sortedKeys(s.Properties) {
		p := s.Properties[k]
		t, nullable, err := g.goType(p, owner+camel(k))
		if err != nil {
			return nil, fmt.Errorf("%s.%s: %w", owner, k, err)
		}
		f := field{json: k, goName: camel(k), doc: p.Description, required: req[k]}
		if o, dup := seen[f.goName]; dup {
			return nil, fmt.Errorf("%s：字段 %s 和 %s 转成 Go 名字重复", owner, k, o)
		}
		seen[f.goName] = k
		if t == owner {
			return nil, fmt.Errorf("%s.%s 直接引用自己", owner, k)
		}
		if req[k] && !nullable {
			f.typ = t
			if strings.HasPrefix(t, "[]") {
				f.kind = "slice"
			} else if strings.HasPrefix(t, "map[") {
				f.kind = "map"
			}
		} else {
			f.typ = "Opt[" + t + "]"
			f.opt = true
		}
		out = append(out, f)
	}
	return out, nil
}

func (g *gen) structDecl(goName string, s *Schema, desc string) (string, error) {
	fs, err := g.fields(goName, s)
	if err != nil {
		return "", err
	}
	var b bytes.Buffer
	if desc == "" {
		desc = "是接口里的一个模型（docs/openapi.json）。"
	}
	comment(&b, "", goName+" "+desc)
	fmt.Fprintf(&b, "type %s struct {\n", goName)
	for _, f := range fs {
		doc := f.doc
		if f.required && !f.opt {
			// 必填的不用说
		} else if f.required {
			doc = strings.TrimSpace(doc + "\n必填，可以是 null。")
		}
		if strings.TrimSpace(doc) != "" {
			comment(&b, "\t", f.goName+" "+strings.TrimSpace(doc))
		}
		fmt.Fprintf(&b, "\t%s %s `json:%q`\n", f.goName, f.typ, f.json)
	}
	b.WriteString("}\n\n")
	fmt.Fprintf(&b, "// MarshalJSON 只写设置了的可选字段：Opt 没设置的不发，oad.Null 的发 null。\nfunc (m %s) MarshalJSON() ([]byte, error) {\n", goName)
	fmt.Fprintf(&b, "\to := make(map[string]any, %d)\n", len(fs))
	for _, f := range fs {
		switch {
		case f.opt:
			fmt.Fprintf(&b, "\tif m.%s.IsSet() {\n\t\to[%q] = m.%s\n\t}\n", f.goName, f.json, f.goName)
		case f.kind == "slice":
			fmt.Fprintf(&b, "\to[%q] = nonNilSlice(m.%s)\n", f.json, f.goName)
		case f.kind == "map":
			fmt.Fprintf(&b, "\to[%q] = nonNilMap(m.%s)\n", f.json, f.goName)
		default:
			fmt.Fprintf(&b, "\to[%q] = m.%s\n", f.json, f.goName)
		}
	}
	b.WriteString("\treturn json.Marshal(o)\n}\n")
	return b.String(), nil
}

// goType：schema → Go 类型。nullable 表示可以是 null（字段要用 Opt）。hint 是 inline 对象要起的名字。
func (g *gen) goType(s *Schema, hint string) (string, bool, error) {
	if s == nil {
		return "any", false, nil
	}
	if s.Ref != "" {
		n := refName(s.Ref)
		if g.schemas[n] == nil {
			return "", false, fmt.Errorf("找不到 %s", s.Ref)
		}
		if err := g.named(n); err != nil {
			return "", false, err
		}
		return typeName(n), false, nil
	}
	if len(s.OneOf) > 0 || len(s.AnyOf) > 0 {
		vs := append(append([]*Schema{}, s.OneOf...), s.AnyOf...)
		var rest []*Schema
		nullable := false
		for _, v := range vs {
			if ts := v.types(); len(ts) == 1 && ts[0] == "null" {
				nullable = true
			} else {
				rest = append(rest, v)
			}
		}
		if len(rest) == 1 {
			t, n2, err := g.goType(rest[0], hint)
			return t, nullable || n2, err
		}
		return "any", nullable, nil
	}
	if len(s.AllOf) == 1 {
		return g.goType(s.AllOf[0], hint)
	}
	ts := s.types()
	nullable := false
	var base []string
	for _, t := range ts {
		if t == "null" {
			nullable = true
		} else {
			base = append(base, t)
		}
	}
	if len(base) != 1 {
		return "any", nullable, nil
	}
	switch base[0] {
	case "string":
		return "string", nullable, nil
	case "boolean":
		return "bool", nullable, nil
	case "integer":
		switch s.Format {
		case "int64":
			return "int64", nullable, nil
		case "int32":
			return "int32", nullable, nil
		}
		return "int", nullable, nil
	case "number":
		return "float64", nullable, nil
	case "array":
		it, _, err := g.goType(s.Items, hint+"Item")
		if err != nil {
			return "", false, err
		}
		return "[]" + it, nullable, nil
	case "object":
		if len(s.Properties) > 0 {
			name := hint
			if _, done := g.decls[name]; !done {
				src, err := g.structDecl(name, s, s.Description)
				if err != nil {
					return "", false, err
				}
				g.decls[name] = src
			}
			return name, nullable, nil
		}
		if len(s.AdditionalProperties) > 0 && string(s.AdditionalProperties) != "true" && string(s.AdditionalProperties) != "{}" {
			var ap Schema
			if err := json.Unmarshal(s.AdditionalProperties, &ap); err == nil {
				vt, _, err := g.goType(&ap, hint+"Value")
				if err != nil {
					return "", false, err
				}
				return "map[string]" + vt, nullable, nil
			}
		}
		return "map[string]any", nullable, nil
	}
	return "any", nullable, nil
}

// ---------- 接口 ----------

type op struct {
	id, method, path string
	o                *Operation
	hint             Hint
	segs             []string
}

var methods = []string{"get", "post", "put", "patch", "delete"}

func (g *gen) operations(t *OpsTable) ([]*op, error) {
	var out []*op
	var errs []string
	for _, p := range sortedKeys(g.spec.Paths) {
		item := g.spec.Paths[p]
		for _, m := range methods {
			raw, ok := item[m]
			if !ok {
				continue
			}
			var o Operation
			if err := json.Unmarshal(raw, &o); err != nil {
				return nil, fmt.Errorf("%s %s: %w", m, p, err)
			}
			if _, skip := t.Skip[o.OperationID]; skip {
				continue
			}
			h, ok := t.Operations[o.OperationID]
			if !ok {
				errs = append(errs, fmt.Sprintf("%s %s（%s）没在 operations.json 里起名字", strings.ToUpper(m), p, o.OperationID))
				continue
			}
			out = append(out, &op{id: o.OperationID, method: strings.ToUpper(m), path: p, o: &o, hint: h, segs: strings.Split(h.Name, ".")})
		}
	}
	if len(errs) > 0 {
		return nil, errors.New(strings.Join(errs, "\n"))
	}
	sort.Slice(out, func(i, j int) bool { return out[i].hint.Name < out[j].hint.Name })
	return out, nil
}

type node struct {
	segs     []string
	children map[string]*node
	ops      []*op
}

func serviceType(segs []string) string {
	var b strings.Builder
	for _, s := range segs {
		b.WriteString(camel(s))
	}
	return b.String() + "Service"
}

func (g *gen) resources(ops []*op) ([]byte, error) {
	root := &node{children: map[string]*node{}}
	for _, o := range ops {
		n := root
		for i, s := range o.segs[:len(o.segs)-1] {
			c := n.children[s]
			if c == nil {
				c = &node{segs: o.segs[:i+1], children: map[string]*node{}}
				n.children[s] = c
			}
			n = c
		}
		n.ops = append(n.ops, o)
	}

	var b bytes.Buffer

	// Client
	b.WriteString("// Client 是 Open Agent Docs 的客户端，用 NewClient 建。资源是字段：client.Pages.Versions.List(ctx, …)。\n// 可以在多个 goroutine 里同时用。\ntype Client struct {\n\tcore *core\n\n")
	for _, k := range sortedKeys(root.children) {
		c := root.children[k]
		fmt.Fprintf(&b, "\t// %s 是 %s.* 的接口。\n\t%s *%s\n", camel(k), k, camel(k), serviceType(c.segs))
	}
	b.WriteString("}\n\n")
	b.WriteString("func (c *Client) initResources() {\n")
	for _, k := range sortedKeys(root.children) {
		fmt.Fprintf(&b, "\tc.%s = %s\n", camel(k), literal(root.children[k]))
	}
	b.WriteString("}\n\n")

	var walk func(n *node) error
	walk = func(n *node) error {
		recv := "c *Client"
		coreExpr := "c.core"
		if len(n.segs) > 0 {
			st := serviceType(n.segs)
			fmt.Fprintf(&b, "// %s 是 %s.* 的接口（client.%s）。\ntype %s struct {\n\tcore *core\n", st, strings.Join(n.segs, "."), goPath(n.segs), st)
			for _, k := range sortedKeys(n.children) {
				c := n.children[k]
				fmt.Fprintf(&b, "\t// %s 是 %s.* 的接口。\n\t%s *%s\n", camel(k), strings.Join(c.segs, "."), camel(k), serviceType(c.segs))
			}
			b.WriteString("}\n\n")
			recv = "r *" + st
			coreExpr = "r.core"
		}
		for _, o := range n.ops {
			if err := g.method(&b, recv, coreExpr, goPath(n.segs), o); err != nil {
				return fmt.Errorf("%s（%s）：%w", o.hint.Name, o.id, err)
			}
		}
		for _, k := range sortedKeys(n.children) {
			if err := walk(n.children[k]); err != nil {
				return err
			}
		}
		return nil
	}
	if err := walk(root); err != nil {
		return nil, err
	}
	var out bytes.Buffer
	header(&out, usedImports(b.Bytes(), "context", "errors", "io", "iter", "net/url", "strconv"))
	out.Write(b.Bytes())
	return out.Bytes(), nil
}

// usedImports 只留代码里真用到的包（解析一遍，看有没有 包名.X 这样的选择表达式）。
func usedImports(src []byte, candidates ...string) []string {
	f, err := parser.ParseFile(token.NewFileSet(), "", append([]byte("package oad\n"), src...), 0)
	if err != nil {
		return candidates // 让后面的 format.Source 报出语法错误
	}
	used := map[string]bool{}
	ast.Inspect(f, func(n ast.Node) bool {
		if sel, ok := n.(*ast.SelectorExpr); ok {
			if id, ok := sel.X.(*ast.Ident); ok {
				used[id.Name] = true
			}
		}
		return true
	})
	var out []string
	for _, c := range candidates {
		if used[c[strings.LastIndex(c, "/")+1:]] {
			out = append(out, c)
		}
	}
	return out
}

func goPath(segs []string) string {
	var p []string
	for _, s := range segs {
		p = append(p, camel(s))
	}
	return strings.Join(p, ".")
}

func literal(n *node) string {
	var b strings.Builder
	fmt.Fprintf(&b, "&%s{core: c.core", serviceType(n.segs))
	for _, k := range sortedKeys(n.children) {
		fmt.Fprintf(&b, ", %s: %s", camel(k), literal(n.children[k]))
	}
	b.WriteString("}")
	return b.String()
}

// pfield 是 XxxParams 里的一个字段。
type pfield struct {
	goName, typ, doc string
	in               string // query / header / body / extra
	wire             string // 参数名 / JSON 字段名
	required         bool
}

func (g *gen) method(b *bytes.Buffer, recv, coreExpr, display string, o *op) error {
	h := o.hint
	name := camel(o.segs[len(o.segs)-1])
	paramsType := camel(strings.ReplaceAll(h.Name, ".", "_")) + "Params"

	// 路径参数，按路径里的顺序
	byName := map[string]*Parameter{}
	for _, p := range o.o.Parameters {
		if p.In == "path" {
			byName[p.Name] = p
		}
	}
	type pathArg struct{ name, goName, typ string }
	var pathArgs []pathArg
	var pathExpr []string
	rest := o.path
	for {
		i := strings.Index(rest, "{")
		if i < 0 {
			if rest != "" {
				pathExpr = append(pathExpr, fmt.Sprintf("%q", rest))
			}
			break
		}
		j := strings.Index(rest, "}")
		if i > 0 {
			pathExpr = append(pathExpr, fmt.Sprintf("%q", rest[:i]))
		}
		pn := rest[i+1 : j]
		p := byName[pn]
		t := "string"
		if p != nil {
			t, _, _ = g.goType(p.Schema, "")
		}
		a := pathArg{pn, lowerCamel(pn), t}
		pathArgs = append(pathArgs, a)
		switch t {
		case "int64":
			pathExpr = append(pathExpr, "strconv.FormatInt("+a.goName+", 10)")
		case "int":
			pathExpr = append(pathExpr, "strconv.Itoa("+a.goName+")")
		default:
			pathExpr = append(pathExpr, "url.PathEscape("+a.goName+")")
		}
		rest = rest[j+1:]
	}

	// 参数结构体
	var pf []pfield
	for _, p := range o.o.Parameters {
		switch p.In {
		case "query":
		case "header":
			if strings.EqualFold(p.Name, "Last-Event-ID") || strings.EqualFold(p.Name, "X-Filename") {
				continue
			}
		default:
			continue
		}
		t, _, err := g.goType(p.Schema, paramsType+camel(p.Name))
		if err != nil {
			return err
		}
		if p.In == "header" {
			t = "string"
		}
		pf = append(pf, pfield{goName: camel(p.Name), typ: t, doc: p.Description, in: p.In, wire: p.Name, required: p.Required})
	}
	sort.SliceStable(pf, func(i, j int) bool { return pf[i].goName < pf[j].goName })

	// 请求体
	var bodyType string    // 拼请求体用的模型
	var bodyFields []field // 拍平到参数里的字段
	bodyWhole := ""        // 不拍平时 Body 字段的类型
	if o.o.RequestBody != nil && h.Upload == "" {
		m, ok := o.o.RequestBody.Content["application/json"]
		if !ok {
			return fmt.Errorf("请求体不是 JSON，又没有 upload 提示")
		}
		s := m.Schema
		// oneOf [X, null]：按 X 拍平
		target := s
		if len(s.OneOf) > 0 {
			var nonNull []*Schema
			for _, v := range s.OneOf {
				if ts := v.types(); !(len(ts) == 1 && ts[0] == "null") {
					nonNull = append(nonNull, v)
				}
			}
			if len(nonNull) == 1 {
				target = nonNull[0]
			}
		}
		r := g.resolve(target)
		if r != nil && len(r.Properties) > 0 && len(r.AllOf) == 0 {
			if target.Ref != "" {
				bodyType = typeName(refName(target.Ref))
				if err := g.named(refName(target.Ref)); err != nil {
					return err
				}
			} else {
				bodyType = paramsType + "Body"
				if _, _, err := g.goType(target, bodyType); err != nil {
					return err
				}
			}
			fs, err := g.fields(bodyType, r)
			if err != nil {
				return err
			}
			bodyFields = fs
		} else {
			t, _, err := g.goType(s, paramsType+"Body")
			if err != nil {
				return err
			}
			bodyWhole = t
		}
	}
	for _, f := range bodyFields {
		pf = append(pf, pfield{goName: f.goName, typ: f.typ, doc: f.doc, in: "body", wire: f.json, required: f.required})
	}
	if bodyWhole != "" {
		pf = append(pf, pfield{goName: "Body", typ: bodyWhole, doc: "请求体（整个 JSON）。", in: "whole"})
	}
	hk, hasHook := hooks[h.Name]
	for _, e := range hk.fields {
		pf = append(pf, pfield{goName: e.name, typ: e.typ, doc: e.doc, in: "extra"})
	}
	seen := map[string]bool{}
	for _, f := range pf {
		if seen[f.goName] {
			return fmt.Errorf("参数 %s 重名（查询参数和请求体字段撞了）", f.goName)
		}
		seen[f.goName] = true
	}
	hasParams := len(pf) > 0

	if hasParams {
		fmt.Fprintf(b, "// %s 是 client.%s 的参数。", paramsType, strings.TrimPrefix(display+"."+name, "."))
		if !anyRequired(pf) {
			b.WriteString("都是可选的，可以传零值。")
		}
		b.WriteString("\n")
		fmt.Fprintf(b, "type %s struct {\n", paramsType)
		for _, f := range pf {
			doc := f.doc
			switch f.in {
			case "query":
				if f.required {
					doc = strings.TrimSpace(doc + "\n查询参数 " + f.wire + "，必填。")
				} else {
					doc = strings.TrimSpace(doc + "\n查询参数 " + f.wire + "，零值不发。")
				}
			case "header":
				doc = strings.TrimSpace(doc + "\n请求头 " + f.wire + "，空字符串不发。")
			case "body":
				if f.required && !strings.HasPrefix(f.typ, "Opt[") {
					doc = strings.TrimSpace(doc + "\n必填。")
				}
			}
			if strings.TrimSpace(doc) != "" {
				comment(b, "\t", f.goName+" "+strings.TrimSpace(doc))
			}
			fmt.Fprintf(b, "\t%s %s\n", f.goName, f.typ)
		}
		b.WriteString("}\n\n")
	}

	// 签名
	var args []string
	args = append(args, "ctx context.Context")
	for _, a := range pathArgs {
		args = append(args, a.goName+" "+a.typ)
	}
	switch h.Upload {
	case "file":
		args = append(args, "name string", "file io.Reader")
	case "csv":
		args = append(args, "csv io.Reader")
	}
	if hasParams {
		args = append(args, "params "+paramsType)
	}

	// 响应
	okCode, resp := successResponse(o.o)
	kind := "json"
	var respType string
	if h.SSE != "" {
		kind = "sse"
	} else if h.Download {
		kind = "download"
	} else if resp == nil || len(resp.Content) == 0 {
		kind = "none"
	} else if m, ok := resp.Content["application/json"]; ok {
		t, _, err := g.goType(m.Schema, "")
		if err != nil {
			return err
		}
		respType = t
	} else {
		return fmt.Errorf("%s 的响应不是 JSON，又没有 download / sse 提示", okCode)
	}
	var unwrapType, unwrapField, itemType string
	var unwrapOpt bool
	if h.Unwrap != "" || h.Paginate != "" {
		k := h.Unwrap
		if k == "" {
			k = h.Paginate
		}
		m := resp.Content["application/json"]
		rs := g.resolve(m.Schema)
		ps := rs.Properties[k]
		if ps == nil {
			return fmt.Errorf("响应里没有 %s", k)
		}
		t, nullable, err := g.goType(ps, respType+camel(k))
		if err != nil {
			return err
		}
		req := false
		for _, r := range rs.Required {
			if r == k {
				req = true
			}
		}
		unwrapType, unwrapField, unwrapOpt = t, camel(k), nullable || !req
		itemType = strings.TrimPrefix(t, "[]")
	}

	var ret string
	switch {
	case kind == "sse" && h.SSE == "workspace":
		ret = "iter.Seq2[Event, error]"
	case kind == "sse":
		ret = "iter.Seq2[*AgentNotification, error]"
	case kind == "download":
		ret = "(io.ReadCloser, error)"
	case kind == "none":
		ret = "error"
	case h.Unwrap != "":
		ret = "(" + unwrapType + ", error)"
	default:
		ret = "(*" + respType + ", error)"
	}

	// 文档
	doc := fmt.Sprintf("%s 对应 %s %s。", name, o.method, o.path)
	if d := strings.TrimSpace(o.o.Description); d != "" {
		doc += "\n" + d
	}
	switch {
	case h.Unwrap != "":
		doc += "\n直接返回响应里的 " + h.Unwrap + "。"
	case h.Paginate != "":
		doc += "\n这是一页；要逐条取完所有页用 " + name + "All。"
	case h.NullOn404:
		doc += "\n没有（404）时返回 nil, nil。"
	case kind == "download":
		doc += "\n返回响应正文，用完要 Close。"
	case kind == "sse" && h.SSE == "workspace":
		doc += "\n逐条给出事件（Event.Data 按事件名解析成模型，见 Event）；断了自动重连。ctx 取消就停；401、403、404 给一个错误后停。"
	case kind == "sse":
		doc += "\n逐条给出通知；断了自动重连，带上收到的最后一个编号接着收。ctx 取消就停；401、403、404 给一个错误后停。"
	}
	if hasHook && h.Name == "pages.create" {
		doc += "\n给了 params.Markdown 就再把它写成正文；建好了但写正文失败时，页面和错误一起返回。"
	}
	comment(b, "", doc)
	fmt.Fprintf(b, "func (%s) %s(%s) %s {\n", recv, name, strings.Join(args, ", "), ret)

	zeroRet := "nil, "
	if kind == "none" {
		zeroRet = ""
	}
	if kind == "sse" && (bodyType != "" || bodyWhole != "" || h.Upload != "") {
		return fmt.Errorf("事件流不支持请求体")
	}

	// 请求
	fmt.Fprintf(b, "\treq := &request{method: %q, path: %s}\n", o.method, strings.Join(pathExpr, " + "))
	hasQuery := false
	for _, f := range pf {
		if f.in == "query" {
			hasQuery = true
		}
	}
	if hasQuery {
		b.WriteString("\tq := url.Values{}\n")
		for _, f := range pf {
			if f.in != "query" {
				continue
			}
			val := "params." + f.goName
			var conv, zero string
			switch f.typ {
			case "string":
				conv, zero = val, `""`
			case "int":
				conv, zero = "strconv.Itoa("+val+")", "0"
			case "int64":
				conv, zero = "strconv.FormatInt("+val+", 10)", "0"
			case "int32":
				conv, zero = "strconv.FormatInt(int64("+val+"), 10)", "0"
			case "bool":
				conv, zero = "strconv.FormatBool("+val+")", "false"
			case "float64":
				conv, zero = "strconv.FormatFloat("+val+", 'f', -1, 64)", "0"
			default:
				return fmt.Errorf("查询参数 %s 的类型 %s 不支持", f.wire, f.typ)
			}
			if f.required {
				fmt.Fprintf(b, "\tq.Set(%q, %s)\n", f.wire, conv)
			} else {
				fmt.Fprintf(b, "\tif %s != %s {\n\t\tq.Set(%q, %s)\n\t}\n", val, zero, f.wire, conv)
			}
		}
		b.WriteString("\treq.query = q\n")
	}
	for _, f := range pf {
		if f.in == "header" {
			fmt.Fprintf(b, "\tif params.%s != \"\" {\n\t\treq.setHeader(%q, params.%s)\n\t}\n", f.goName, f.wire, f.goName)
		}
	}
	switch h.Upload {
	case "file":
		b.WriteString("\treq.setHeader(\"X-Filename\", encodeURIComponent(name))\n\treq.setReader(file, \"application/octet-stream\")\n")
	case "csv":
		b.WriteString("\treq.setReader(csv, \"text/csv; charset=utf-8\")\n")
	}
	if bodyType != "" {
		var parts []string
		for _, f := range bodyFields {
			parts = append(parts, f.goName+": params."+f.goName)
		}
		fmt.Fprintf(b, "\tif err := req.setJSON(%s{%s}); err != nil {\n\t\treturn %serr\n\t}\n", bodyType, strings.Join(parts, ", "), zeroRet)
	} else if bodyWhole != "" {
		fmt.Fprintf(b, "\tif err := req.setJSON(params.Body); err != nil {\n\t\treturn %serr\n\t}\n", zeroRet)
	}

	switch kind {
	case "sse":
		if h.SSE == "workspace" {
			fmt.Fprintf(b, "\treturn %s.workspaceEvents(ctx, req)\n}\n\n", coreExpr)
		} else {
			fmt.Fprintf(b, "\treturn %s.agentStream(ctx, req)\n}\n\n", coreExpr)
		}
		return nil
	case "download":
		fmt.Fprintf(b, "\treturn %s.download(ctx, req)\n}\n\n", coreExpr)
		return nil
	case "none":
		fmt.Fprintf(b, "\treturn %s.do(ctx, req, nil)\n}\n\n", coreExpr)
		return nil
	}
	fmt.Fprintf(b, "\tvar out %s\n", respType)
	fmt.Fprintf(b, "\tif err := %s.do(ctx, req, &out); err != nil {\n", coreExpr)
	if h.NullOn404 {
		b.WriteString("\t\tif errors.Is(err, ErrNotFound) {\n\t\t\treturn nil, nil\n\t\t}\n")
	}
	b.WriteString("\t\treturn nil, err\n\t}\n")
	switch {
	case h.Unwrap != "":
		if unwrapOpt {
			fmt.Fprintf(b, "\treturn out.%s.Value(), nil\n}\n\n", unwrapField)
		} else {
			fmt.Fprintf(b, "\treturn out.%s, nil\n}\n\n", unwrapField)
		}
	case hasHook && hk.after != "":
		fmt.Fprintf(b, "\treturn %s.%s(ctx, &out, params)\n}\n\n", coreExpr, hk.after)
	default:
		b.WriteString("\treturn &out, nil\n}\n\n")
	}

	if h.Paginate != "" {
		if !hasParams || !seen["Cursor"] {
			return fmt.Errorf("paginate 但参数里没有 Cursor")
		}
		var callArgs []string
		callArgs = append(callArgs, "ctx")
		for _, a := range pathArgs {
			callArgs = append(callArgs, a.goName)
		}
		callArgs = append(callArgs, "p")
		items := "page." + unwrapField
		if unwrapOpt {
			items += ".Value()"
		}
		fmt.Fprintf(b, "// %sAll 逐条取完 %s 的所有页（自动带上 cursor 取下一页）。break 出循环就不再取了；出错时给一次错误后停。\n", name, name)
		fmt.Fprintf(b, "func (%s) %sAll(%s) iter.Seq2[%s, error] {\n", recv, name, strings.Join(args, ", "), itemType)
		fmt.Fprintf(b, "\treturn paginate(params.Cursor, func(cursor string) ([]%s, string, error) {\n", itemType)
		b.WriteString("\t\tp := params\n\t\tp.Cursor = cursor\n")
		fmt.Fprintf(b, "\t\tpage, err := %s.%s(%s)\n", recvVar(recv), name, strings.Join(callArgs, ", "))
		b.WriteString("\t\tif err != nil {\n\t\t\treturn nil, \"\", err\n\t\t}\n")
		fmt.Fprintf(b, "\t\treturn %s, page.NextCursor.Value(), nil\n\t})\n}\n\n", items)
	}
	return nil
}

func recvVar(recv string) string { return recv[:strings.Index(recv, " ")] }

func anyRequired(pf []pfield) bool {
	for _, f := range pf {
		if f.required && !strings.HasPrefix(f.typ, "Opt[") {
			return true
		}
	}
	return false
}

func successResponse(o *Operation) (string, *struct {
	Description string           `json:"description"`
	Content     map[string]Media `json:"content"`
}) {
	for _, k := range sortedKeys(o.Responses) {
		if strings.HasPrefix(k, "2") {
			return k, o.Responses[k]
		}
	}
	return "", nil
}

// ---------- 事件 ----------

func (g *gen) events() ([]byte, error) {
	var b bytes.Buffer
	header(&b, []string{"encoding/json"})
	type ev struct{ name, constName, typ string }
	var evs []ev
	for _, name := range sortedKeys(g.spec.Webhooks) {
		post := g.spec.Webhooks[name]["post"]
		if post == nil || post.RequestBody == nil {
			continue
		}
		m, ok := post.RequestBody.Content["application/json"]
		if !ok {
			continue
		}
		t, _, err := g.goType(m.Schema, camel(strings.ReplaceAll(name, ".", "_"))+"Event")
		if err != nil {
			return nil, fmt.Errorf("事件 %s：%w", name, err)
		}
		evs = append(evs, ev{name, "Event" + camel(strings.ReplaceAll(name, ".", "_")), t})
	}
	b.WriteString("// 事件名（工作区事件流和 Agent 的 webhook）。Event.Data 的类型：\n//\n")
	for _, e := range evs {
		dt := e.typ
		if !strings.HasPrefix(dt, "map[") && dt != "any" {
			dt = "*" + dt
		}
		fmt.Fprintf(&b, "//   - %s：%s\n", e.name, dt)
	}
	b.WriteString("//\n// 别的（以后加的）事件名：json.RawMessage。\nconst (\n")
	for _, e := range evs {
		fmt.Fprintf(&b, "\t%s = %q\n", e.constName, e.name)
	}
	b.WriteString(")\n\n")
	b.WriteString("// decodeEventData 按事件名把 data 解析成模型；不认识的给原始 JSON。\nfunc decodeEventData(name string, data []byte) (any, error) {\n\tswitch name {\n")
	for _, e := range evs {
		fmt.Fprintf(&b, "\tcase %s:\n", e.constName)
		if strings.HasPrefix(e.typ, "map[") || e.typ == "any" {
			fmt.Fprintf(&b, "\t\tvar v %s\n\t\terr := json.Unmarshal(data, &v)\n\t\treturn v, err\n", e.typ)
		} else {
			fmt.Fprintf(&b, "\t\tv := new(%s)\n\t\terr := json.Unmarshal(data, v)\n\t\treturn v, err\n", e.typ)
		}
	}
	b.WriteString("\t}\n\treturn json.RawMessage(data), nil\n}\n")
	return b.Bytes(), nil
}
