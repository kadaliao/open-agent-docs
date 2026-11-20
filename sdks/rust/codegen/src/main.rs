//! `cargo run -p codegen`：重新生成；`-- --check`：只检查是不是最新的。
//! 试别的输入时可以指定路径：`-- --openapi x.json --operations y.json --out dir/`。
fn main() {
    let mut paths = codegen::Paths::default_paths();
    let mut check = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().map(std::path::PathBuf::from).expect("缺参数值");
        match a.as_str() {
            "--check" => check = true,
            "--openapi" => paths.openapi = val(),
            "--operations" => paths.operations = val(),
            "--out" => paths.out_dir = val(),
            other => {
                eprintln!("不认识的参数：{other}");
                std::process::exit(2);
            }
        }
    }
    match codegen::run(&paths, check) {
        Ok(stale) if stale.is_empty() => println!("生成结果是最新的"),
        Ok(stale) if check => {
            eprintln!(
                "这些生成的文件过时了：{}；跑 cargo run -p codegen",
                stale.join("、")
            );
            std::process::exit(1);
        }
        Ok(stale) => println!("重新生成了：{}", stale.join("、")),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
}
