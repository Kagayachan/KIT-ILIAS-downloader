use std::{path::Path, sync::Arc};

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};

use crate::{
	ilias::Object,
	process_gracefully,
	queue::spawn,
	util::{file_escape, save_debug_html},
	ILIAS_URL,
};

use super::{ILIAS, LINKS, URL};

/// One video entry: the title link carries the player URL in `data-legacy-href`, because `href`
/// now points at the external KIT media portal instead.
static VIDEO_LINK: Lazy<Selector> = Lazy::new(|| Selector::parse(".c-entity__primary-identifier a").unwrap());
static PAGE_SIZE: Lazy<Regex> = Lazy::new(|| Regex::new(r"page_size=(\d+)").unwrap());

/// Videos and, if the list looks truncated, the link to a larger page size.
fn parse_video_page(source: &str) -> (Vec<(String, String)>, Option<String>) {
	let html = Html::parse_document(source);
	let videos: Vec<(String, String)> = html
		.select(&VIDEO_LINK)
		.filter_map(|link| {
			let url = link.value().attr("data-legacy-href")?;
			let title = link.text().collect::<String>().trim().to_owned();
			if title.is_empty() {
				return None;
			}
			// video::download prepends ILIAS_URL, so hand it a relative URL
			Some((title, url.strip_prefix(ILIAS_URL).unwrap_or(url).to_owned()))
		})
		.collect();
	// ponytail: one hop to the largest offered page size, which covers 50 videos; a collection
	// bigger than that needs real pagination over the page= links
	let larger = html
		.select(&LINKS)
		.filter_map(|l| l.value().attr("href"))
		.filter_map(|h| {
			PAGE_SIZE
				.captures(h)
				.map(|c| (c[1].parse::<usize>().unwrap_or(0), h.to_owned()))
		})
		.filter(|(size, _)| *size > videos.len())
		.max_by_key(|(size, _)| *size)
		.map(|(_, href)| href);
	(videos, larger)
}

pub async fn download(path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	if ilias.opt.no_videos {
		return Ok(());
	}
	// The event list is rendered straight into this page. Older Opencast plugin versions needed
	// three requests to reach an async table endpoint; that endpoint no longer exists.
	let source = ilias.download(&url.url).await?.text().await?;
	let (mut videos, larger) = parse_video_page(&source);
	if let Some(larger) = larger {
		if !videos.is_empty() {
			log!(1, "Requesting full video list: {}", larger);
			let source = ilias.download(&larger).await?.text().await?;
			videos = parse_video_page(&source).0;
		}
	}
	if videos.is_empty() {
		if ilias.opt.debug_html {
			save_debug_html(&ilias.opt.output, &format!("xoct_{}", url.ref_id), &source).await?;
		}
		return Err(anyhow!("no videos found in collection (re-run with --debug-html)"));
	}
	for (title, video_url) in videos {
		log!(1, "Found video: {}", title);
		let mut path = path.to_owned();
		path.push(format!("{}.mp4", file_escape(&title)));
		let video = Object::Video {
			url: URL::raw(video_url),
		};
		let ilias = Arc::clone(&ilias);
		spawn(process_gracefully(ilias, path, video));
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::parse_video_page;

	#[test]
	fn reads_title_and_player_url_from_entity_markup() {
		let page = r#"<ul>
			<li><div class="c-entity__primary-identifier"><a
				href="https://ilias-medien.bibliothek.kit.edu/details/abc"
				data-legacy-href="https://ilias.studium.kit.edu/ilias.php?cmdClass=xoctPlayerGUI&amp;cmd=streamVideo&amp;eid=abc"
				>Lecture 1 - Introduction</a></div></li>
			<li><a href="ilias.php?ref_id=1&amp;page_size=50">50</a></li>
		</ul>"#;
		let (videos, larger) = parse_video_page(page);
		// the player URL comes from data-legacy-href, not href, and is made relative for video.rs
		assert_eq!(
			videos,
			vec![(
				"Lecture 1 - Introduction".to_owned(),
				"ilias.php?cmdClass=xoctPlayerGUI&cmd=streamVideo&eid=abc".to_owned()
			)]
		);
		// a page size larger than the number of videos found is offered as a follow-up
		assert_eq!(larger.as_deref(), Some("ilias.php?ref_id=1&page_size=50"));
	}

	#[test]
	fn ignores_pages_without_entities() {
		let (videos, _) = parse_video_page(r#"<div class="il-item">Posteingang</div>"#);
		assert!(videos.is_empty());
	}
}
