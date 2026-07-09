use std::{path::Path, sync::Arc};

use anyhow::Result;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use tokio::fs;

use crate::util::{file_escape, write_stream_to_file};

use super::{ILIAS, URL};

fn extension_from_content_type(content_type: &str) -> Option<&'static str> {
	let mime = content_type.split(';').next()?.trim().to_ascii_lowercase();
	match mime.as_str() {
		"application/pdf" => Some("pdf"),
		"application/vnd.ms-powerpoint" => Some("ppt"),
		"application/vnd.openxmlformats-officedocument.presentationml.presentation" => Some("pptx"),
		"application/msword" => Some("doc"),
		"application/vnd.openxmlformats-officedocument.wordprocessingml.document" => Some("docx"),
		"application/vnd.ms-excel" => Some("xls"),
		"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Some("xlsx"),
		"application/zip" => Some("zip"),
		"image/jpeg" => Some("jpg"),
		"image/png" => Some("png"),
		"text/plain" => Some("txt"),
		_ => None,
	}
}

fn extension_from_content_disposition(value: &str) -> Option<String> {
	for part in value.split(';') {
		let part = part.trim();
		if let Some(filename) = part.strip_prefix("filename=") {
			let filename = filename.trim_matches('"');
			if let Some(ext) = Path::new(filename).extension() {
				return ext.to_str().map(|s| s.to_ascii_lowercase());
			}
		}
	}
	None
}

fn path_with_extension(path: &Path, resp: &reqwest::Response) -> std::path::PathBuf {
	if path.extension().is_some() {
		return path.to_path_buf();
	}
	if let Some(cd) = resp.headers().get(CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()) {
		if let Some(ext) = extension_from_content_disposition(cd) {
			return path.with_extension(ext);
		}
	}
	if let Some(ct) = resp.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()) {
		if let Some(ext) = extension_from_content_type(ct) {
			return path.with_extension(ext);
		}
	}
	path.to_path_buf()
}

pub async fn download(path: &Path, relative_path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	if ilias.opt.skip_files {
		return Ok(());
	}
	let data = ilias.download(&url.url).await?;
	let path = path_with_extension(path, &data);
	let relative_path = if path.extension().is_some() && relative_path.extension().is_none() {
		let mut parts: Vec<_> = relative_path.components().collect();
		if let Some(std::path::Component::Normal(name)) = parts.pop() {
			let name = file_escape(&format!(
				"{}.{}",
				name.to_string_lossy(),
				path.extension().unwrap().to_string_lossy()
			));
			let mut new_path = std::path::PathBuf::new();
			for part in parts {
				new_path.push(part.as_os_str());
			}
			new_path.push(name);
			new_path
		} else {
			relative_path.to_path_buf()
		}
	} else {
		relative_path.to_path_buf()
	};
	if !ilias.opt.force && fs::metadata(&path).await.is_ok() {
		log!(2, "Skipping download, file exists already");
		return Ok(());
	}
	log!(0, "Writing {}", relative_path.to_string_lossy());
	write_stream_to_file(&path, data.bytes_stream()).await?;
	Ok(())
}
