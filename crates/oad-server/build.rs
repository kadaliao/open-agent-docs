// 网页构建产物变了（包括第一次出现）时重新打包进二进制。
fn main() {
    println!("cargo:rerun-if-changed=../../apps/web/dist");
}
