use std::{path::Path, sync::Arc};

use anyhow::{anyhow, Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Url;
use scraper::{Html, Selector};

use crate::{
	ilias::Object,
	process_gracefully,
	queue::spawn,
	util::{file_escape, save_debug_html},
	ILIAS_URL,
};

use super::{ILIAS, LINKS, URL};

static A_TARGET_BLANK: Lazy<Selector> = Lazy::new(|| Selector::parse(r#"a[target="_blank"]"#).unwrap());
static VIDEO_ROWS: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilTableOuter > div > table > tbody > tr").unwrap());
static TABLE_CELLS: Lazy<Selector> = Lazy::new(|| Selector::parse("td").unwrap());
static ILIAS_PHP_URL: Lazy<Regex> = Lazy::new(|| Regex::new(r#"ilias\.php\?[^"'\s<>]+"#).unwrap());

/// Pick the asynchronous xoct event list link out of raw page source. Matching the whole URL with
/// one pattern was too brittle: query parameter order carries no meaning yet the old pattern
/// required a fixed one, it hardcoded the cmdNode width, and it wanted a literal `&` where ILIAS
/// writes `&amp;` inside href attributes. Substring tests avoid all three.
fn find_xoct_list_url(html: &str) -> Option<String> {
	ILIAS_PHP_URL
		.find_iter(html)
		.map(|m| m.as_str().replace("&amp;", "&"))
		.find(|url| {
			let url = url.to_ascii_lowercase();
			url.contains("cmdclass=xocteventgui") && url.contains("async=true")
		})
}

const NO_ENTRIES: &str = "Keine Einträge";

pub async fn download(path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	if ilias.opt.no_videos {
		return Ok(());
	}
	let full_url = {
		let html = ilias.download(&url.url).await?.text().await?;
		let list_url = match find_xoct_list_url(&html) {
			Some(url) => url,
			None => {
				// nothing to go on otherwise: keep the page so the next attempt is not another guess
				if ilias.opt.debug_html {
					save_debug_html(&ilias.opt.output, &format!("xoct_{}", url.ref_id), &html).await?;
				}
				return Err(anyhow!("failed to find xoct event link (re-run with --debug-html)"));
			},
		};
		let full_list_url = format!("{}{}", ILIAS_URL, list_url);

		// first find the link to full video list
		log!(1, "Loading {}", full_list_url);
		let data = ilias.download(&full_list_url).await?;
		let html = data.text().await?;
		let html = Html::parse_fragment(&html);
		html.select(&LINKS)
			.filter_map(|link| link.value().attr("href"))
			.filter(|href| href.contains("trows=800"))
			.map(|x| x.to_string())
			.next()
			.context("video list link not found")?
	};
	log!(1, "Rewriting {}", full_url);
	let mut full_url = Url::parse(&format!("{}{}", ILIAS_URL, full_url))?;
	let mut query_parameters = full_url
		.query_pairs()
		.map(|(x, y)| (x.into_owned(), y.into_owned()))
		.collect::<Vec<_>>();
	for (key, value) in &mut query_parameters {
		match key.as_ref() {
			"cmd" => *value = "asyncGetTableGUI".into(),
			"cmdClass" => *value = "xocteventgui".into(),
			_ => {},
		}
	}
	query_parameters.push(("cmdMode".into(), "asynch".into()));
	full_url
		.query_pairs_mut()
		.clear()
		.extend_pairs(&query_parameters)
		.finish();
	log!(1, "Loading {}", full_url);
	let data = ilias.download(full_url.as_str()).await?;
	let html = data.text().await?;
	let html = Html::parse_fragment(&html);
	for row in html.select(&VIDEO_ROWS) {
		let link = row.select(&A_TARGET_BLANK).next();
		if link.is_none() {
			if !row.text().any(|x| x == NO_ENTRIES) {
				warning!(format => "table row without link in {}", url.url);
			}
			continue;
		}
		let link = link.unwrap();
		let mut cells = row.select(&TABLE_CELLS);
		if let Some(title) = cells.nth(2) {
			let title = title.text().collect::<String>();
			let title = title.trim();
			if title.starts_with("<div") {
				continue;
			}
			let mut path = path.to_owned();
			path.push(format!("{}.mp4", file_escape(title)));
			log!(1, "Found video: {}", title);
			let video = Object::Video {
				url: URL::raw(link.value().attr("href").context("video link without href")?.to_owned()),
			};
			let ilias = Arc::clone(&ilias);
			spawn(process_gracefully(ilias, path, video));
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::find_xoct_list_url;

	#[test]
	fn finds_xoct_list_link_regardless_of_escaping_or_order() {
		// as emitted in page source: &amp; escaping, 11-character cmdNode
		let escaped = r#"<a href="ilias.php?baseClass=ilobjplugindispatchgui&amp;cmdNode=xu:nx:80:6k&amp;cmdClass=xoctEventGUI&amp;ref_id=2943594&amp;async=true">x</a>"#;
		assert_eq!(
			find_xoct_list_url(escaped).as_deref(),
			Some("ilias.php?baseClass=ilobjplugindispatchgui&cmdNode=xu:nx:80:6k&cmdClass=xoctEventGUI&ref_id=2943594&async=true")
		);
		// a different parameter order must still match: order carries no meaning
		let reordered = "ilias.php?ref_id=1&async=true&cmdClass=xoctEventGUI&cmdNode=xu:nx&baseClass=ilobjplugindispatchgui";
		assert_eq!(find_xoct_list_url(reordered).as_deref(), Some(reordered));
		// a non-async xoct link is not the list endpoint
		assert!(find_xoct_list_url("ilias.php?cmdClass=xoctEventGUI&ref_id=1").is_none());
		// some other plugin's link must not be picked up
		assert!(find_xoct_list_url("ilias.php?cmdClass=ilObjGroupGUI&ref_id=1&async=true").is_none());
	}
}
