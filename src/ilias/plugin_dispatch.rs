use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Url;
use scraper::{Html, Selector};

use crate::{ilias::Object, process_gracefully, queue::spawn, util::file_escape, ILIAS_URL};

use super::{ILIAS, LINKS, URL};

static A_TARGET_BLANK: Lazy<Selector> = Lazy::new(|| Selector::parse(r#"a[target="_blank"]"#).unwrap());
static VIDEO_ROWS: Lazy<Selector> = Lazy::new(|| Selector::parse(".ilTableOuter > div > table > tbody > tr").unwrap());
static TABLE_CELLS: Lazy<Selector> = Lazy::new(|| Selector::parse("td").unwrap());
static LIST_URL: Lazy<Regex> = Lazy::new(|| {
	// Matched against the raw HTML, where ILIAS writes &amp; inside href attributes, so both
	// separators have to be accepted. cmdNode is also not a fixed width -- every one on this
	// deployment is 11 characters while the previous pattern hardcoded 9, which matched nothing.
	Regex::new(
		r#"(?i)ilias\.php\?baseClass=ilobjplugindispatchgui(?:&|&amp;)cmdNode=[^&"'\s<>]+(?:&|&amp;)cmdClass=xoctEventGUI(?:&|&amp;)ref_id=\d+(?:&|&amp;)async=true"#,
	)
	.unwrap()
});

const NO_ENTRIES: &str = "Keine Einträge";

pub async fn download(path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	if ilias.opt.no_videos {
		return Ok(());
	}
	let full_url = {
		let html = ilias.download(&url.url).await?.text().await?;
		let list_url = LIST_URL.find(&html).context("failed to find xoct event link")?.as_str();
		// the match comes straight out of the HTML source, so undo the attribute escaping
		let full_list_url = format!("{}{}", ILIAS_URL, list_url.replace("&amp;", "&"));

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
	use super::LIST_URL;

	#[test]
	fn list_url_matches_escaped_and_variable_length_cmdnode() {
		// as it appears in the page source: &amp; escaping, 11-character cmdNode
		let escaped = r#"<a href="ilias.php?baseClass=ilobjplugindispatchgui&amp;cmdNode=xu:nx:80:6k&amp;cmdClass=xoctEventGUI&amp;ref_id=2943594&amp;async=true">"#;
		assert_eq!(
			LIST_URL.find(escaped).map(|m| m.as_str().replace("&amp;", "&")),
			Some("ilias.php?baseClass=ilobjplugindispatchgui&cmdNode=xu:nx:80:6k&cmdClass=xoctEventGUI&ref_id=2943594&async=true".to_owned())
		);
		// unescaped, and a cmdNode of a different length
		let plain = "ilias.php?baseClass=ilobjplugindispatchgui&cmdNode=xu:nx&cmdClass=xoctEventGUI&ref_id=1&async=true";
		assert!(LIST_URL.is_match(plain));
		// a different plugin's link must not match
		assert!(!LIST_URL.is_match(
			"ilias.php?baseClass=ilobjplugindispatchgui&cmdNode=xu:nx&cmdClass=ilObjGroupGUI&ref_id=1&async=true"
		));
	}
}
