//! 提交进仓库的 `open-agent-docs/src/generated/` 和 openapi.json、operations.json 对得上。
#[test]
fn generated_code_is_up_to_date() {
    let paths = codegen::Paths::default_paths();
    let stale = codegen::run(&paths, true).expect("生成失败");
    assert!(
        stale.is_empty(),
        "这些生成的文件过时了：{}；在 sdks/rust 下跑 cargo run -p codegen",
        stale.join("、")
    );
}
