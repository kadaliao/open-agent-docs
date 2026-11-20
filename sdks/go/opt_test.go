package oad

import (
	"encoding/json"
	"testing"
)

func TestOptJSONRequest(t *testing.T) {
	cases := []struct {
		in   PatchPage
		want string
	}{
		{PatchPage{}, `{}`},
		{PatchPage{After: Null[string]()}, `{"after":null}`},
		{PatchPage{After: Some("b1"), Title: Some("")}, `{"after":"b1","title":""}`},
	}
	for _, c := range cases {
		b, err := json.Marshal(c.in)
		if err != nil {
			t.Fatal(err)
		}
		if string(b) != c.want {
			t.Errorf("%+v → %s，应该是 %s", c.in, b, c.want)
		}
	}
	// 必填的数组没给：发 []，不发 null
	b, _ := json.Marshal(AddRows{})
	if string(b) != `{"rows":[]}` {
		t.Fatalf("AddRows{} → %s", b)
	}
	// 嵌套
	b, _ = json.Marshal(AddRows{Rows: []AddRow{{Body: Some("x")}, {}}})
	if string(b) != `{"rows":[{"body":"x"},{}]}` {
		t.Fatalf("嵌套 → %s", b)
	}
}

func TestOptJSONResponse(t *testing.T) {
	var p Page
	if err := json.Unmarshal([]byte(`{"id":"p","title":"T","parent_id":null,"icon":"📄"}`), &p); err != nil {
		t.Fatal(err)
	}
	if !p.ParentID.IsNull() || p.ParentID.Valid() || !p.ParentID.IsSet() {
		t.Fatal("null 没解对")
	}
	if v, ok := p.Icon.Get(); !ok || v != "📄" || p.Icon.Or("x") != "📄" {
		t.Fatal("值没解对")
	}
	if p.DeletedAt.IsSet() || p.DeletedAt.Or(-1) != -1 || p.DeletedAt.Ptr() != nil {
		t.Fatal("没给的应该是没设置")
	}
	// 来回一趟不变
	b, _ := json.Marshal(p)
	var q Page
	json.Unmarshal(b, &q)
	if q.ParentID != p.ParentID || q.Icon != p.Icon || q.DeletedAt != p.DeletedAt {
		t.Fatalf("来回一趟变了：%s", b)
	}
}

func TestOptHelpers(t *testing.T) {
	if Some(3).String() != "3" || Null[int]().String() != "null" || (Opt[int]{}).String() != "<unset>" {
		t.Fatal("String")
	}
	if !(Opt[int]{}).IsZero() || Some(0).IsZero() {
		t.Fatal("IsZero")
	}
	if p := Some(5).Ptr(); p == nil || *p != 5 {
		t.Fatal("Ptr")
	}
}
