//! CRDT 对比实验（Loro / Yrs）的 Rust 端。
//!
//!   build  <out>          base.json → loro.base、yrs.base（Rust 建的文档，给 JS 端检查能不能读）
//!   agent  <out>          在 base 上按 edits.json 改（模拟 Agent 按段改），导出增量 loro.agent、yrs.agent
//!   merge  <out>          base + JS 端的 *.user + *.agent 两种顺序合并，结果写 *.final.json
//!   bench  <base.json>    建文档、导出、导入、读、按段改、回到旧版本的耗时和大小
mod loro_doc;
mod pm;
mod yrs_doc;

use loro::{ExportMode, LoroDoc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use yrs::updates::decoder::Decode;
use yrs::{Doc, ReadTxn, StateVector, Transact, Update};

const BUILD_PEER: u64 = 1;
const AGENT_PEER: u64 = 2;

fn styles(out: &Path) -> HashMap<String, String> {
    serde_json::from_slice(&fs::read(out.join("styles.json")).unwrap()).unwrap()
}

fn load_json(p: &Path) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}

/// 比较前去掉节点上值为 null 的属性（两个绑定写入时都跳过 null，读出来由 schema 补默认值）。
fn norm(v: &Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.iter()
                .filter_map(|(k, x)| {
                    if k == "attrs" && o.get("type") != Some(&Value::String("text".into())) && !o.contains_key("text") {
                        let a: serde_json::Map<String, Value> =
                            x.as_object()?.iter().filter(|(_, y)| !y.is_null()).map(|(k, y)| (k.clone(), y.clone())).collect();
                        if a.is_empty() { None } else { Some((k.clone(), Value::Object(a))) }
                    } else {
                        Some((k.clone(), norm(x)))
                    }
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
        _ => v.clone(),
    }
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn loro_from(out: &Path, peer: u64, files: &[&str]) -> LoroDoc {
    let d = loro_doc::new_doc(peer, &styles(out));
    for f in files {
        d.import(&fs::read(out.join(f)).unwrap()).unwrap();
    }
    d
}

fn yrs_from(out: &Path, client: u64, files: &[&str]) -> Doc {
    let d = yrs_doc::new_doc(client, true);
    for f in files {
        let u = Update::decode_v1(&fs::read(out.join(f)).unwrap()).unwrap();
        d.transact_mut().apply_update(u).unwrap();
    }
    d
}

fn apply_edits_loro(d: &LoroDoc, edits: &[Value]) {
    for e in edits {
        let id = e["id"].as_str().unwrap();
        let nodes = e["nodes"].as_array().cloned().unwrap_or_default();
        let ok = match e["op"].as_str().unwrap() {
            "replace" => loro_doc::replace_block(d, id, &nodes),
            "insert_after" => loro_doc::insert_after(d, id, &nodes),
            "delete" => loro_doc::delete_block(d, id),
            _ => false,
        };
        assert!(ok, "loro: 找不到块 {id}");
    }
}

fn apply_edits_yrs(d: &Doc, edits: &[Value]) {
    for e in edits {
        let id = e["id"].as_str().unwrap();
        let nodes = e["nodes"].as_array().cloned().unwrap_or_default();
        let ok = match e["op"].as_str().unwrap() {
            "replace" => yrs_doc::replace_block(d, id, &nodes),
            "insert_after" => yrs_doc::insert_after(d, id, &nodes),
            "delete" => yrs_doc::delete_block(d, id),
            _ => false,
        };
        assert!(ok, "yrs: 找不到块 {id}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("");
    let arg = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "../out".into()));
    match cmd {
        "build" => build(&arg),
        "agent" => agent(&arg),
        "merge" => merge(&arg),
        "bench" => bench(&arg),
        "profile" => profile_loro(&arg),
        _ => eprintln!("用法：crdt-spike build|agent|merge <out> / bench <base.json>"),
    }
}

fn build(out: &Path) {
    let base = load_json(&out.join("base.json"));
    let d = loro_doc::new_doc(BUILD_PEER, &styles(out));
    loro_doc::build(&d, &base);
    fs::write(out.join("loro.base"), d.export(ExportMode::Snapshot).unwrap()).unwrap();
    let y = yrs_doc::new_doc(BUILD_PEER, true);
    yrs_doc::build(&y, &base);
    fs::write(out.join("yrs.base"), y.transact().encode_state_as_update_v1(&StateVector::default())).unwrap();
    // Rust 自己读回来也要一样
    assert!(norm(&loro_doc::read(&d)) == norm(&base), "loro 读回来和 base 不一样");
    assert!(norm(&yrs_doc::read(&y)) == norm(&base), "yrs 读回来和 base 不一样");
    println!("build: 两边都写好了，Rust 读回来和 base.json 一致");
}

fn agent(out: &Path) {
    let edits: Vec<Value> = load_json(&out.join("edits.json")).as_array().cloned().unwrap();
    let d = loro_from(out, AGENT_PEER, &["loro.base"]);
    let vv = d.oplog_vv();
    apply_edits_loro(&d, &edits);
    fs::write(out.join("loro.agent"), d.export(ExportMode::updates(&vv)).unwrap()).unwrap();
    let y = yrs_from(out, AGENT_PEER, &["yrs.base"]);
    let sv = y.transact().state_vector();
    apply_edits_yrs(&y, &edits);
    fs::write(out.join("yrs.agent"), y.transact().encode_diff_v1(&sv)).unwrap();
    println!("agent: {} 处改动，增量 loro {} B、yrs {} B", edits.len(),
        fs::metadata(out.join("loro.agent")).unwrap().len(), fs::metadata(out.join("yrs.agent")).unwrap().len());
}

fn merge(out: &Path) {
    let a = loro_doc::read(&loro_from(out, 9, &["loro.base", "loro.user", "loro.agent"]));
    let b = loro_doc::read(&loro_from(out, 9, &["loro.base", "loro.agent", "loro.user"]));
    assert_eq!(a, b, "loro 两种顺序合并结果不一样");
    fs::write(out.join("loro.final.json"), serde_json::to_vec(&a).unwrap()).unwrap();
    let a = yrs_doc::read(&yrs_from(out, 9, &["yrs.base", "yrs.user", "yrs.agent"]));
    let b = yrs_doc::read(&yrs_from(out, 9, &["yrs.base", "yrs.agent", "yrs.user"]));
    assert_eq!(a, b, "yrs 两种顺序合并结果不一样");
    fs::write(out.join("yrs.final.json"), serde_json::to_vec(&a).unwrap()).unwrap();
    println!("merge: 两边两种顺序结果一致，写好 *.final.json");
}

/// 找前 n 个顶层段落，造“改一个词”的编辑。
fn paragraph_edits(base: &Value, n: usize) -> Vec<Value> {
    base["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["type"] == "paragraph")
        .take(n)
        .map(|b| {
            let mut nb = b.clone();
            let t = nb["content"][0]["text"].as_str().unwrap().to_string();
            nb["content"][0]["text"] = format!("{t}【Agent 改过】").into();
            serde_json::json!({"op": "replace", "id": b["attrs"]["id"], "nodes": [nb]})
        })
        .collect()
}

fn bench(base_path: &Path) {
    let out = base_path.parent().unwrap();
    let base = load_json(base_path);
    let blocks = base["content"].as_array().unwrap().len();
    let edits = paragraph_edits(&base, 100);
    println!("== {blocks} 个顶层块，{} 次按段改 ==", edits.len());

    // ---- Loro ----
    let t = Instant::now();
    let d = loro_doc::new_doc(BUILD_PEER, &styles(out));
    loro_doc::build(&d, &base);
    let build_ms = ms(t);
    let t = Instant::now();
    let snap = d.export(ExportMode::Snapshot).unwrap();
    let export_ms = ms(t);
    let t = Instant::now();
    let d2 = loro_doc::new_doc(5, &styles(out));
    d2.import(&snap).unwrap();
    let import_ms = ms(t);
    let t = Instant::now();
    let _ = loro_doc::read(&d2);
    let read_ms = ms(t);
    let v0 = d2.oplog_frontiers();
    let vv0 = d2.oplog_vv();
    let t = Instant::now();
    apply_edits_loro(&d2, &edits);
    let edit_ms = ms(t) / edits.len() as f64;
    let upd = d2.export(ExportMode::updates(&vv0)).unwrap();
    let snap2 = d2.export(ExportMode::Snapshot).unwrap();
    let t = Instant::now();
    d2.checkout(&v0).unwrap();
    let old = loro_doc::read(&d2);
    let checkout_ms = ms(t);
    assert!(norm(&old) == norm(&base), "loro 回到旧版本和 base 不一样");
    d2.try_checkout_to_latest().unwrap();
    let t = Instant::now();
    let d3 = loro_doc::new_doc(6, &styles(out));
    d3.import(&snap2).unwrap();
    let _ = loro_doc::read(&d3);
    let reload_ms = ms(t);
    println!(
        "loro: 建 {build_ms:.1}ms | 快照 {} KB（导出 {export_ms:.1}ms）| 导入 {import_ms:.1}ms | 读成 JSON {read_ms:.1}ms | 按段改 {edit_ms:.3}ms/次 | 100 次增量 {} KB | 改后快照 {} KB，导入+读 {reload_ms:.1}ms | 回到旧版本+读 {checkout_ms:.1}ms",
        snap.len() / 1024, upd.len() / 1024, snap2.len() / 1024
    );

    // ---- Yrs ----
    let t = Instant::now();
    let y = yrs_doc::new_doc(BUILD_PEER, true);
    yrs_doc::build(&y, &base);
    let build_ms = ms(t);
    let t = Instant::now();
    let snap = y.transact().encode_state_as_update_v1(&StateVector::default());
    let export_ms = ms(t);
    let snap_v2 = y.transact().encode_state_as_update_v2(&StateVector::default());
    let t = Instant::now();
    let y2 = yrs_doc::new_doc(5, true);
    y2.transact_mut().apply_update(Update::decode_v1(&snap).unwrap()).unwrap();
    let import_ms = ms(t);
    let t = Instant::now();
    let _ = yrs_doc::read(&y2);
    let read_ms = ms(t);
    let sv0 = y2.transact().state_vector();
    let s0 = y2.transact().snapshot();
    let t = Instant::now();
    apply_edits_yrs(&y2, &edits);
    let edit_ms = ms(t) / edits.len() as f64;
    let upd = y2.transact().encode_diff_v1(&sv0);
    let snap2 = y2.transact().encode_state_as_update_v1(&StateVector::default());
    let t = Instant::now();
    let mut enc = yrs::updates::encoder::EncoderV1::new();
    y2.transact().encode_state_from_snapshot(&s0, &mut enc).unwrap();
    let old_doc = yrs_doc::new_doc(7, true);
    old_doc.transact_mut().apply_update(Update::decode_v1(&yrs::updates::encoder::Encoder::to_vec(enc)).unwrap()).unwrap();
    let old = yrs_doc::read(&old_doc);
    let checkout_ms = ms(t);
    assert!(norm(&old) == norm(&base), "yrs 回到旧版本和 base 不一样");
    let t = Instant::now();
    let y3 = yrs_doc::new_doc(6, true);
    y3.transact_mut().apply_update(Update::decode_v1(&snap2).unwrap()).unwrap();
    let _ = yrs_doc::read(&y3);
    let reload_ms = ms(t);
    println!(
        "yrs:  建 {build_ms:.1}ms | 快照 v1 {} KB / v2 {} KB（导出 {export_ms:.1}ms）| 导入 {import_ms:.1}ms | 读成 JSON {read_ms:.1}ms | 按段改 {edit_ms:.3}ms/次 | 100 次增量 {} KB | 改后快照 {} KB，导入+读 {reload_ms:.1}ms | 回到旧版本+读 {checkout_ms:.1}ms",
        snap.len() / 1024, snap_v2.len() / 1024, upd.len() / 1024, snap2.len() / 1024
    );
}

#[allow(dead_code)]
pub fn profile_loro(base_path: &Path) {
    let out = base_path.parent().unwrap();
    let base = load_json(base_path);
    let d = loro_doc::new_doc(BUILD_PEER, &styles(out));
    loro_doc::build(&d, &base);
    let snap = d.export(ExportMode::Snapshot).unwrap();
    let d2 = loro_doc::new_doc(5, &styles(out));
    d2.import(&snap).unwrap();
    let t = Instant::now();
    let v = d2.get_deep_value();
    println!("get_deep_value（整份，不含 mark）{:.1}ms", ms(t));
    drop(v);
    let t = Instant::now();
    let _ = loro_doc::read(&d2);
    println!("第一次 read {:.1}ms", ms(t));
    let t = Instant::now();
    let _ = loro_doc::read(&d2);
    println!("第二次 read {:.1}ms", ms(t));
    let t = Instant::now();
    let _ = loro_doc::read(&d);
    println!("在建它的那份上 read {:.1}ms", ms(t));
}
