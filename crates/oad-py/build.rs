fn main() {
    // macOS 上不经 maturin、直接 cargo build（工作区一起编）时也能链接：Python 的符号到加载时再找
    pyo3_build_config::add_extension_module_link_args();
}
