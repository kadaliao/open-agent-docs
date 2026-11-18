//! 把网页构建产物（apps/web/dist）打进二进制：能压缩的文件预先做好 brotli 和 gzip 两份（不再放原文，
//! 不支持压缩的客户端由服务器现解 gzip），图片、字体这类本来就压缩过的放原文。生成一张表给 src/web.rs 用。
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn compressible(mime: &str) -> bool {
    mime.starts_with("text/")
        || mime.contains("javascript")
        || mime.contains("json")
        || mime.contains("xml")
        || mime == "application/wasm"
        || mime == "image/svg+xml"
        || mime == "application/manifest+json"
}

fn main() {
    let dist =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../apps/web/dist");
    println!("cargo:rerun-if-changed={}", dist.display());
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let blobs = out.join("web");
    let _ = std::fs::remove_dir_all(&blobs);
    std::fs::create_dir_all(&blobs).unwrap();
    let mut list = Vec::new();
    files(&dist, &mut list);
    list.sort();
    let mut table = String::from("&[\n");
    for (i, p) in list.iter().enumerate() {
        println!("cargo:rerun-if-changed={}", p.display());
        let rel = p
            .strip_prefix(&dist)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        // 网页正在重新构建时文件可能刚被删掉：跳过（构建完 dist 变了会再跑一次）
        let Ok(data) = std::fs::read(p) else { continue };
        let mime = mime_guess::from_path(p).first_or_octet_stream().to_string();
        let mime = if mime.starts_with("text/") || mime.contains("javascript") {
            format!("{mime}; charset=utf-8")
        } else {
            mime
        };
        let etag: String = {
            use sha2::Digest;
            sha2::Sha256::digest(&data)[..12]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect()
        };
        let (mut raw, mut br, mut gz) =
            ("None".to_string(), "None".to_string(), "None".to_string());
        let mut done = false;
        if compressible(&mime) && data.len() > 256 {
            let mut b = Vec::new();
            {
                let mut w = brotli::CompressorWriter::new(&mut b, 1 << 16, 11, 22);
                w.write_all(&data).unwrap();
            }
            let mut g = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
            g.write_all(&data).unwrap();
            let g = g.finish().unwrap();
            // 压不下多少的就放原文
            if g.len() * 10 < data.len() * 9 {
                let (bp, gp) = (blobs.join(format!("{i}.br")), blobs.join(format!("{i}.gz")));
                std::fs::write(&bp, &b).unwrap();
                std::fs::write(&gp, &g).unwrap();
                br = format!("Some(include_bytes!({:?}))", bp.display().to_string());
                gz = format!("Some(include_bytes!({:?}))", gp.display().to_string());
                done = true;
            }
        }
        if !done {
            let rp = blobs.join(format!("{i}.raw"));
            std::fs::write(&rp, &data).unwrap();
            raw = format!("Some(include_bytes!({:?}))", rp.display().to_string());
        }
        writeln!(
            table,
            "    Asset {{ path: {rel:?}, mime: {mime:?}, etag: {etag:?}, len: {}, raw: {raw}, br: {br}, gz: {gz} }},",
            data.len()
        )
        .unwrap();
    }
    table.push(']');
    std::fs::write(out.join("web_assets.rs"), table).unwrap();
}
