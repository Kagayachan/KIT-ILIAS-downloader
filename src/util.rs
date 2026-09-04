// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::Context;
use bytes::Bytes;
use futures::TryStreamExt;
use tokio::fs::File as AsyncFile;
use tokio::io::{AsyncRead, AsyncWriteExt, BufWriter};
use tokio_util::io::StreamReader;

use std::io;
use std::path::{Path, PathBuf};

use crate::{Result, ILIAS_URL};

/// Prepends a doctype and a base URL to the HTML fragment.
pub fn wrap_html(html_fragment: &str) -> String {
	format!("<!DOCTYPE html>\n<base href=\"{}\">{}", ILIAS_URL, html_fragment)
}

pub async fn write_stream_to_file(
	path: &Path,
	stream: impl futures::Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
) -> Result<()> {
	let mut reader = StreamReader::new(stream.map_err(|x| io::Error::new(io::ErrorKind::Other, x)));
	write_file_data(&path, &mut reader).await?;
	Ok(())
}

/// Write all data to the specified path. Will overwrite previous file data.
///
/// The data goes to a sibling `.part` file that is renamed into place only once it is complete.
/// Writing straight to the destination would leave a truncated file behind on an interrupted
/// transfer, and every later run treats a file that exists as already downloaded, so that
/// truncated file would be skipped forever instead of being fetched again.
pub async fn write_file_data<R: ?Sized>(path: impl AsRef<Path>, data: &mut R) -> Result<()>
where
	R: AsyncRead + Unpin,
{
	let path = path.as_ref();
	// append rather than replace the extension, so "x.mp4" and "x.zip" cannot collide
	let mut partial = path.as_os_str().to_owned();
	partial.push(".part");
	let partial = PathBuf::from(partial);

	let file = AsyncFile::create(&partial).await.context("failed to create file")?;
	let mut file = BufWriter::new(file);
	tokio::io::copy(data, &mut file)
		.await
		.context("failed to write to file")?;
	// tokio::fs::File defers writes to a blocking pool, so the flush has to happen before the
	// rename rather than being left to the drop
	file.flush().await.context("failed to flush file")?;
	drop(file);

	// ponytail: a .part left by a killed process is simply overwritten on the next attempt;
	// cleaning up would need a signal handler, which is not worth it for a stray temp file
	tokio::fs::rename(&partial, path)
		.await
		.context("failed to move completed file into place")?;
	Ok(())
}

/// Create a directory. Does not error if the directory already exists.
pub async fn create_dir(path: &Path) -> Result<()> {
	if let Err(e) = tokio::fs::create_dir(&path).await {
		if e.kind() != tokio::io::ErrorKind::AlreadyExists {
			return Err(e.into());
		}
	}
	Ok(())
}

#[cfg(not(target_os = "windows"))]
const INVALID: &[char] = &['/', '\\'];
#[cfg(target_os = "windows")]
const INVALID: &[char] = &['/', '\\', ':', '<', '>', '"', '|', '?', '*'];

pub fn file_escape(s: &str) -> String {
	s.replace(INVALID, "-")
}

/// Save HTML to `<output>/.debug/<name>.html` when troubleshooting ILIAS page parsing.
pub async fn save_debug_html(output: &Path, name: &str, html: &str) -> Result<()> {
	let dir = output.join(".debug");
	create_dir(&dir).await?;
	let safe_name = file_escape(name).replace(' ', "_");
	let path = dir.join(format!("{}.html", safe_name));
	write_file_data(&path, &mut html.as_bytes()).await?;
	log!(1, "Saved debug HTML to {}", path.display());
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::write_file_data;
	use std::io;
	use std::pin::Pin;
	use std::task::{Context, Poll};
	use tokio::io::{AsyncRead, ReadBuf};

	/// Yields some bytes, then fails: a transfer that dies partway through.
	struct FailsPartway(usize);

	impl AsyncRead for FailsPartway {
		fn poll_read(mut self: Pin<&mut Self>, _: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
			if self.0 == 0 {
				return Poll::Ready(Err(io::Error::new(io::ErrorKind::ConnectionReset, "connection reset")));
			}
			self.0 -= 1;
			buf.put_slice(b"partial data");
			Poll::Ready(Ok(()))
		}
	}

	#[tokio::test]
	async fn completed_write_lands_at_the_final_path() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("Lecture 1 - Introduction.mp4");
		write_file_data(&path, &mut &b"video"[..]).await.unwrap();

		assert_eq!(std::fs::read(&path).unwrap(), b"video");
		// the temporary file must not be left behind
		assert!(!dir.path().join("Lecture 1 - Introduction.mp4.part").exists());
	}

	#[tokio::test]
	async fn interrupted_write_leaves_nothing_at_the_final_path() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("big.mp4");
		assert!(write_file_data(&path, &mut FailsPartway(3)).await.is_err());

		// the crucial guarantee: a later run must not mistake a truncated transfer for a
		// complete file, which it would if anything existed at the final path
		assert!(!path.exists());
	}
}
