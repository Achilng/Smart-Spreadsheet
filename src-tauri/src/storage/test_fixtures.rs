//! 测试共用的图片夹具：生成可解码、带 NovelAI 风格文本元数据的真实 PNG。
//! 各模块测试统一从这里取带 NovelAI 元数据的样例图。

use std::fs;
use std::path::{Path, PathBuf};

/// 生成一张可被 image crate 正常解码的小尺寸 PNG，并在 IHDR 后插入
/// 带正确 CRC 的 tEXt 块（`Description` 必有，`Comment` 可选）。
pub(crate) fn metadata_png_bytes(description: &str, comment: Option<&str>) -> Vec<u8> {
    let mut chunks = vec![("Description", description.to_string())];
    if let Some(comment) = comment {
        chunks.push(("Comment", comment.to_string()));
    }
    metadata_png_with_text_chunks(chunks)
}

/// 与 `metadata_png_bytes` 相同，但额外写入 `Source` 文本块（作画模型名）。
pub(crate) fn metadata_png_bytes_with_source(
    description: &str,
    comment: Option<&str>,
    source: &str,
) -> Vec<u8> {
    let mut chunks = vec![("Description", description.to_string())];
    if let Some(comment) = comment {
        chunks.push(("Comment", comment.to_string()));
    }
    chunks.push(("Source", source.to_string()));
    metadata_png_with_text_chunks(chunks)
}

fn metadata_png_with_text_chunks(chunks: Vec<(&str, String)>) -> Vec<u8> {
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(16, 16)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .expect("encode fixture png");
    let encoded = encoded.into_inner();

    // IHDR 块结束位置：签名 8 字节 + 长度/类型 8 字节 + 数据 13 字节 + CRC 4 字节。
    let insert_at = 8 + 8 + 13 + 4;
    let mut output = encoded[..insert_at].to_vec();
    for (keyword, text) in &chunks {
        push_text_chunk(&mut output, keyword, text);
    }
    output.extend_from_slice(&encoded[insert_at..]);
    output
}

fn push_text_chunk(png: &mut Vec<u8>, keyword: &str, text: &str) {
    let mut data = keyword.as_bytes().to_vec();
    data.push(0);
    data.extend_from_slice(text.as_bytes());

    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let type_and_data: Vec<u8> = [b"tEXt".as_slice(), &data].concat();
    let mut crc = flate2::Crc::new();
    crc.update(&type_and_data);
    png.extend_from_slice(&type_and_data);
    png.extend_from_slice(&crc.sum().to_be_bytes());
}

pub(crate) fn write_metadata_png(path: &Path, description: &str) {
    fs::write(path, metadata_png_bytes(description, None)).expect("write fixture png");
}

/// 在 `parent` 下创建含 `count` 张带元数据 PNG 的文件夹，返回文件夹路径。
/// 各图 Description 互不相同，保证内容哈希不去重。
pub(crate) fn sample_image_folder(parent: &Path, count: usize) -> PathBuf {
    let folder = parent.join("sample-images");
    fs::create_dir_all(&folder).expect("create fixture folder");
    for index in 1..=count {
        write_metadata_png(
            &folder.join(format!("sample-{index}.png")),
            &format!("best quality, artist:样例{index}"),
        );
    }
    folder
}
