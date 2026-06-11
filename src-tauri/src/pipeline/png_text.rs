//! PNG 文本 chunk 读取器（自 Novelai工具 移植）。
//! 通过 Seek 跳过 IDAT 等像素数据，只读取 tEXt / zTXt / iTXt。

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use flate2::read::ZlibDecoder;
use thiserror::Error;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_TEXT_CHUNK_BYTES: u32 = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum PngTextError {
    #[error("PNG 读取失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(&'static str),
}

pub fn read_png_text_chunks(
    path: impl AsRef<Path>,
) -> Result<BTreeMap<String, String>, PngTextError> {
    let file = File::open(path.as_ref())?;
    read_png_text_chunks_from_reader(file)
}

pub fn read_png_text_chunks_from_reader(
    mut reader: impl Read + Seek,
) -> Result<BTreeMap<String, String>, PngTextError> {
    let mut signature = [0_u8; 8];
    reader.read_exact(&mut signature)?;
    if &signature != PNG_SIGNATURE {
        return Err(PngTextError::Invalid("不是有效的 PNG 文件"));
    }

    let mut chunks = BTreeMap::new();
    loop {
        let mut header = [0_u8; 8];
        match reader.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error.into()),
        }

        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let chunk_type = [header[4], header[5], header[6], header[7]];

        let is_text_chunk = matches!(&chunk_type, b"tEXt" | b"zTXt" | b"iTXt");
        if !is_text_chunk {
            if &chunk_type == b"IEND" {
                break;
            }
            // 像素数据等无关 chunk 直接跳过（数据 + 4 字节 CRC），避免整文件读取。
            reader.seek(SeekFrom::Current(i64::from(length) + 4))?;
            continue;
        }

        if length > MAX_TEXT_CHUNK_BYTES {
            return Err(PngTextError::Invalid("PNG 文本 chunk 过大"));
        }

        let mut data = vec![0_u8; length as usize];
        reader.read_exact(&mut data)?;
        reader.seek(SeekFrom::Current(4))?;

        match &chunk_type {
            b"tEXt" => {
                if let Some((keyword, text)) = parse_text_chunk(&data) {
                    chunks.entry(keyword).or_insert(text);
                }
            }
            b"zTXt" => {
                if let Some((keyword, text)) = parse_compressed_text_chunk(&data)? {
                    chunks.entry(keyword).or_insert(text);
                }
            }
            b"iTXt" => {
                if let Some((keyword, text)) = parse_international_text_chunk(&data)? {
                    chunks.entry(keyword).or_insert(text);
                }
            }
            _ => unreachable!("non-text chunks are skipped above"),
        }
    }

    Ok(chunks)
}

fn parse_text_chunk(data: &[u8]) -> Option<(String, String)> {
    let (keyword, text) = split_once_nul(data)?;
    Some((decode_text(keyword), decode_text(text)))
}

fn parse_compressed_text_chunk(data: &[u8]) -> Result<Option<(String, String)>, PngTextError> {
    let Some((keyword, rest)) = split_once_nul(data) else {
        return Ok(None);
    };
    if rest.is_empty() {
        return Ok(None);
    }

    let compression_method = rest[0];
    if compression_method != 0 {
        return Err(PngTextError::Invalid("zTXt 使用了不支持的压缩方法"));
    }

    let text = inflate_zlib(&rest[1..])
        .map_err(|_| PngTextError::Invalid("无法解压 zTXt 文本"))?;
    Ok(Some((decode_text(keyword), decode_text(&text))))
}

fn parse_international_text_chunk(
    data: &[u8],
) -> Result<Option<(String, String)>, PngTextError> {
    let Some((keyword, rest)) = split_once_nul(data) else {
        return Ok(None);
    };
    if rest.len() < 2 {
        return Ok(None);
    }

    let compression_flag = rest[0];
    let compression_method = rest[1];
    if compression_flag > 1 {
        return Err(PngTextError::Invalid("iTXt 使用了不支持的压缩标记"));
    }
    if compression_flag == 1 && compression_method != 0 {
        return Err(PngTextError::Invalid("iTXt 使用了不支持的压缩方法"));
    }

    let Some((_language_tag, after_language)) = split_once_nul(&rest[2..]) else {
        return Ok(None);
    };
    let Some((_translated_keyword, text_bytes)) = split_once_nul(after_language) else {
        return Ok(None);
    };

    let text = if compression_flag == 1 {
        inflate_zlib(text_bytes).map_err(|_| PngTextError::Invalid("无法解压 iTXt 文本"))?
    } else {
        text_bytes.to_vec()
    };

    Ok(Some((decode_text(keyword), decode_text(&text))))
}

fn split_once_nul(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let index = data.iter().position(|byte| *byte == 0)?;
    Some((&data[..index], &data[index + 1..]))
}

fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut decoder = ZlibDecoder::new(Cursor::new(data));
    let mut output = Vec::new();
    decoder.read_to_end(&mut output)?;
    Ok(output)
}

fn decode_text(data: &[u8]) -> String {
    String::from_utf8(data.to_vec())
        .unwrap_or_else(|_| data.iter().map(|byte| *byte as char).collect())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use super::read_png_text_chunks_from_reader;

    #[test]
    fn reads_text_chunks() {
        let png = png_with_chunks(vec![chunk(b"tEXt", b"Description\0artist:test")]);

        let chunks = read_png_text_chunks_from_reader(Cursor::new(png)).unwrap();

        assert_eq!(chunks.get("Description"), Some(&"artist:test".to_string()));
    }

    #[test]
    fn reads_compressed_text_chunks() {
        let compressed = zlib_bytes(b"{\"prompt\":\"artist:z\"}");
        let mut data = b"Comment\0".to_vec();
        data.push(0);
        data.extend(compressed);
        let png = png_with_chunks(vec![chunk(b"zTXt", &data)]);

        let chunks = read_png_text_chunks_from_reader(Cursor::new(png)).unwrap();

        assert_eq!(
            chunks.get("Comment"),
            Some(&"{\"prompt\":\"artist:z\"}".to_string())
        );
    }

    #[test]
    fn reads_international_text_chunks() {
        let mut data = b"Comment\0".to_vec();
        data.push(0);
        data.push(0);
        data.extend(b"zh-CN\0");
        data.extend(b"Comment\0");
        data.extend("中文提示词".as_bytes());
        let png = png_with_chunks(vec![chunk(b"iTXt", &data)]);

        let chunks = read_png_text_chunks_from_reader(Cursor::new(png)).unwrap();

        assert_eq!(chunks.get("Comment"), Some(&"中文提示词".to_string()));
    }

    #[test]
    fn skips_large_non_text_chunks_and_reads_following_text() {
        let big_idat = vec![0_u8; 4 * 1024 * 1024];
        let png = png_with_chunks(vec![
            chunk(b"IDAT", &big_idat),
            chunk(b"tEXt", b"Description\0artist:after-idat"),
        ]);

        let chunks = read_png_text_chunks_from_reader(Cursor::new(png)).unwrap();

        assert_eq!(
            chunks.get("Description"),
            Some(&"artist:after-idat".to_string())
        );
    }

    fn png_with_chunks(chunks: Vec<Vec<u8>>) -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        for chunk in chunks {
            png.extend(chunk);
        }
        png.extend(chunk(b"IEND", &[]));
        png
    }

    fn chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        output.extend((data.len() as u32).to_be_bytes());
        output.extend(chunk_type);
        output.extend(data);
        output.extend(0_u32.to_be_bytes());
        output
    }

    fn zlib_bytes(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }
}
