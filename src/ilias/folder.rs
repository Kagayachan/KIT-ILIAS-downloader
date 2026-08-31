use std::{collections::HashSet, path::Path, sync::Arc};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::{
	process_gracefully,
	queue::spawn,
	util::{file_escape, write_file_data},
};

use super::{ILIAS, URL};

static EXPAND_LINK: Lazy<Regex> = Lazy::new(|| Regex::new("expand=\\d").unwrap());

/// Upper bound on session-expansion hops for a single folder.
const MAX_EXPANSIONS: usize = 32;

pub async fn download(path: &Path, ilias: Arc<ILIAS>, url: &URL) -> Result<()> {
	// Expand all sessions. Every hop re-fetches the whole folder at the expanded URL and throws
	// away the page it came from, so this costs one request -- 7.5s at the default --rate -- per
	// collapsed session. Run with -v to see how many a folder actually takes.
	let mut followed: HashSet<String> = HashSet::new();
	let mut expanded: Option<URL> = None;
	let content = loop {
		let content = ilias.get_course_content(expanded.as_ref().unwrap_or(url)).await?;
		// link format: ilias.php?ref_id=1943526&expand=2602906&cmd=view&cmdClass=ilobjfoldergui&cmdNode=x1:nk&baseClass=ilrepositorygui#lg_div_1948579_pref_1943526
		// Skipping links already followed keeps this terminating even if ILIAS re-collapses the
		// previous session instead of accumulating them, which would otherwise ping-pong forever.
		let next = content
			.2
			.iter()
			.find(|href| EXPAND_LINK.is_match(href) && !followed.contains(*href))
			.cloned();
		let Some(href) = next else {
			break content;
		};
		// ponytail: a flat cap, not a proof that ILIAS runs out of sessions; the visited set
		// already rules out a cycle, this only bounds a pathological folder
		if followed.len() >= MAX_EXPANSIONS {
			warning!(format => "stopped expanding {} after {} sessions", path.display(), followed.len());
			break content;
		}
		log!(1, "Expanding session {} of {}", followed.len() + 1, path.display());
		followed.insert(href.clone());
		expanded = Some(URL::from_href(&href)?);
	};
	if !followed.is_empty() {
		log!(1, "Expanded {} sessions in {}", followed.len(), path.display());
	}

	if ilias.opt.save_ilias_pages {
		if let Some(s) = content.1.as_ref() {
			let path = path.join("folder.html");
			write_file_data(&path, &mut s.as_bytes())
				.await
				.context("failed to write folder page html")?;
		}
	}

	let mut names = HashSet::new();
	for item in content.0 {
		let item = item?;
		let item_name = file_escape(ilias.course_names.get(item.name()).map(|x| &**x).unwrap_or(item.name()));
		if names.contains(&item_name) {
			warning!(format => "folder {} contains duplicated folder {:?}", path.display(), item_name);
		}
		names.insert(item_name.clone());
		let path = path.join(item_name);
		let ilias = Arc::clone(&ilias);
		spawn(process_gracefully(ilias, path, item));
	}
	Ok(())
}
