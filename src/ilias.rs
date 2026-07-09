// SPDX-License-Identifier: GPL-3.0-or-later

use std::{collections::HashMap, error::Error as _, io::Write, sync::Arc};

use anyhow::{anyhow, Context, Result};
use cookie_store::CookieStore;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{Client, IntoUrl, Proxy, Url};
use reqwest_cookie_store::CookieStoreMutex;
use scraper::{ElementRef, Html, Selector};
use serde_json::json;

use crate::{cli::Opt, iliasignore::IliasIgnore, queue, util::{save_debug_html, wrap_html}, ILIAS_URL};

pub mod course;
pub mod exercise;
pub mod file;
pub mod folder;
pub mod forum;
pub mod plugin_dispatch;
pub mod thread;
pub mod video;
pub mod weblink;

static LINKS: Lazy<Selector> = Lazy::new(|| Selector::parse("a").unwrap());
static ALERT_DANGER: Lazy<Selector> = Lazy::new(|| Selector::parse("div.alert-danger, .il_ItemAlertProperty").unwrap());
static IL_CONTENT_CONTAINER: Lazy<Selector> = Lazy::new(|| Selector::parse("#il_center_col, #ilContentContainer").unwrap());
static BLOCK_FAVORITES: Lazy<Selector> = Lazy::new(|| Selector::parse("#block_pditems_0").unwrap());
static BLOCK_DASH_FAV: Lazy<Selector> = Lazy::new(|| Selector::parse("#block_dash_fav_0").unwrap());
static ITEM_PROP: Lazy<Selector> = Lazy::new(|| Selector::parse("span.il_ItemProperty").unwrap());
static ITEM_PROPERTIES: Lazy<Selector> = Lazy::new(|| Selector::parse(".il_ItemProperties").unwrap());
static CONTAINER_ITEMS: Lazy<Selector> =
	Lazy::new(|| Selector::parse("div.il_ContainerListItem, .il-std-item").unwrap());
static CONTAINER_ITEM_TITLE: Lazy<Selector> =
	Lazy::new(|| Selector::parse("a.il_ContainerItemTitle, .il-item-title > a, .il-item-title a").unwrap());
static CARD_TITLE_LINK: Lazy<Selector> = Lazy::new(|| Selector::parse(".card-title a").unwrap());
static CONTENT_PAGE_FILE_LINK: Lazy<Selector> =
	Lazy::new(|| Selector::parse("a.ilc_flist_a_FileListItemLink").unwrap());
static CONTENT_PAGE_SIZE_SUFFIX: Lazy<Regex> =
	Lazy::new(|| Regex::new(r"\([\d,.]+ [MK]B\)").unwrap());
static TAB_VIEW_CONTENT: Lazy<Selector> = Lazy::new(|| Selector::parse("#tab_view_content").unwrap());
static GOTO_PATH: Lazy<Regex> = Lazy::new(|| Regex::new(r"goto\.php/([a-z]+)/(\d+)").unwrap());
static GOTO_FILE_DOWNLOAD: Lazy<Regex> = Lazy::new(|| Regex::new(r"goto\.php/file/(\d+)/download").unwrap());

pub struct ILIAS {
	pub opt: Opt,
	pub ignore: IliasIgnore,
	client: Client,
	cookies: Arc<CookieStoreMutex>,
	pub course_names: HashMap<String, String>,
}

/// Returns true if the error is caused by:
/// "http2 error: protocol error: not a result of an error"
fn error_is_http2(error: &reqwest::Error) -> bool {
	error
		.source() // hyper::Error
		.and_then(|x| x.source()) // h2::Error
		.and_then(|x| x.downcast_ref::<h2::Error>())
		.and_then(|x| x.reason())
		.map(|x| x == h2::Reason::NO_ERROR)
		.unwrap_or(false)
}

impl ILIAS {
	// TODO: de-duplicate the logic below
	pub async fn with_session(
		opt: Opt,
		session: Arc<CookieStoreMutex>,
		ignore: IliasIgnore,
		course_names: HashMap<String, String>,
	) -> Result<Self> {
		let mut builder = Client::builder()
			.cookie_provider(Arc::clone(&session))
			.user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")));
		if let Some(proxy) = opt.proxy.as_ref() {
			let proxy = Proxy::all(proxy)?;
			builder = builder.proxy(proxy);
		}
		let client = builder
			// timeout is infinite by default
			.build()?;
		info!("Re-using previous session cookies..");
		Ok(ILIAS {
			opt,
			ignore,
			client,
			cookies: session,
			course_names,
		})
	}

	pub async fn login(
		opt: Opt,
		user: &str,
		pass: &str,
		ignore: IliasIgnore,
		course_names: HashMap<String, String>,
	) -> Result<Self> {
		let cookie_store = CookieStore::default();
		let cookie_store = reqwest_cookie_store::CookieStoreMutex::new(cookie_store);
		let cookie_store = std::sync::Arc::new(cookie_store);
		let mut builder = Client::builder()
			.cookie_provider(Arc::clone(&cookie_store))
			.user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")));
		if let Some(proxy) = opt.proxy.as_ref() {
			let proxy = Proxy::all(proxy)?;
			builder = builder.proxy(proxy);
		}
		let client = builder
			// timeout is infinite by default
			.build()?;
		let this = ILIAS {
			opt,
			ignore,
			client,
			cookies: cookie_store,
			course_names,
		};
		info!("Logging into ILIAS using KIT account..");
		let session_establishment = this
			.client
			.post("https://ilias.studium.kit.edu/Shibboleth.sso/Login")
			.form(&json!({
				"sendLogin": "1",
				"idp_selection": "https://idp.scc.kit.edu/idp/shibboleth",
				"target": "/shib_login.php?target=",
				"home_organization_selection": "Mit KIT-Account anmelden"
			}))
			.send()
			.await?;
		let url = session_establishment.url().clone();
		let text = session_establishment.text().await?;
		let dom_sso = Html::parse_document(text.as_str());
		let csrf_token = dom_sso
			.select(&Selector::parse(r#"input[name="csrf_token"]"#).unwrap())
			.next()
			.context("no CSRF token found")?
			.value()
			.attr("value")
			.context("no CSRF token value")?;
		info!("Logging into Shibboleth..");
		let login_response = this
			.client
			.post(url)
			.form(&json!({
				"j_username": user,
				"j_password": pass,
				"_eventId_proceed": "",
				"csrf_token": csrf_token,
			}))
			.send()
			.await?
			.text()
			.await?;
		let dom = Html::parse_document(&login_response);
		let saml = Selector::parse(r#"input[name="SAMLResponse"]"#).unwrap();
		let saml = dom
			.select(&saml)
			.next()
			.context("no SAML response, incorrect password?")?;
		let relay_state = Selector::parse(r#"input[name="RelayState"]"#).unwrap();
		let relay_state = dom.select(&relay_state).next().context("no relay state")?;
		info!("Logging into ILIAS..");
		this.client
			.post("https://ilias.studium.kit.edu/Shibboleth.sso/SAML2/POST")
			.form(&json!({
				"SAMLResponse": saml.value().attr("value").context("no SAML value")?,
				"RelayState": relay_state.value().attr("value").context("no RelayState value")?
			}))
			.send()
			.await?;
		success!("Logged in!");
		Ok(this)
	}

	pub async fn save_session(&self) -> Result<()> {
		let session_path = self.opt.output.join(".iliassession");
		let mut writer = std::fs::File::create(session_path)
			.map(std::io::BufWriter::new)
			.unwrap();
		let store = self.cookies.lock().map_err(|x| anyhow!("{}", x))?;
		// save all cookies, including session cookies
		for cookie in store.iter_unexpired().map(serde_json::to_string) {
			writeln!(writer, "{}", cookie?)?;
		}
		writer.flush()?;
		Ok(())
	}

	pub async fn download(&self, url: &str) -> Result<reqwest::Response> {
		queue::get_request_ticket().await;
		log!(2, "Downloading {}", url);
		let url = if url.starts_with("http://") || url.starts_with("https://") {
			url.to_owned()
		} else if url.starts_with("ilias.studium.kit.edu") {
			format!("https://{}", url)
		} else {
			format!("{}{}", ILIAS_URL, url)
		};
		for attempt in 1..10 {
			let result = self.client.get(url.clone()).send().await;
			match result {
				Ok(x) => return Ok(x),
				Err(e) if attempt <= 3 && error_is_http2(&e) => {
					warning!(1; "encountered HTTP/2 NO_ERROR, retrying download..");
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
		unreachable!()
	}

	pub async fn head<U: IntoUrl>(&self, url: U) -> Result<reqwest::Response, reqwest::Error> {
		queue::get_request_ticket().await;
		let url = url.into_url()?;
		for attempt in 1..10 {
			let result = self.client.head(url.clone()).send().await;
			match result {
				Ok(x) => return Ok(x),
				Err(e) if attempt <= 3 && error_is_http2(&e) => {
					warning!(1; "encountered HTTP/2 NO_ERROR, retrying HEAD request..");
					continue;
				},
				Err(e) => return Err(e),
			}
		}
		unreachable!()
	}

	pub fn is_error_response(html: &Html) -> bool {
		html.select(&ALERT_DANGER).next().is_some()
	}

	pub async fn get_html(&self, url: &str) -> Result<Html> {
		let resp = self.download(url).await?;
		if resp
			.url()
			.query()
			.map(|x| x.contains("reloadpublic=1") || x.contains("cmd=force_login"))
			.unwrap_or(false)
		{
			return Err(anyhow!("not logged in / session expired"));
		}
		let text = resp.text().await?;
		let html = Html::parse_document(&text);
		if ILIAS::is_error_response(&html) {
			Err(anyhow!("ILIAS error"))
		} else {
			Ok(html)
		}
	}

	pub async fn get_html_fragment(&self, url: &str) -> Result<Html> {
		let text = self.download(url).await?.text().await?;
		let html = Html::parse_fragment(&text);
		if ILIAS::is_error_response(&html) {
			Err(anyhow!("ILIAS error"))
		} else {
			Ok(html)
		}
	}

	fn should_skip_link(href: &str) -> bool {
		let href_lower = href.to_ascii_lowercase();
		href_lower.contains("ilmailgui")
			|| (href_lower.contains("cmd=manage") && href_lower.contains("ilpdselecteditemsblockgui"))
			|| href_lower.contains("cmd=jumptomemberships")
			|| href_lower.contains("block_type=pditems")
	}

	fn find_list_item_parent<'a>(mut link: ElementRef<'a>) -> ElementRef<'a> {
		while let Some(parent) = link.parent().and_then(ElementRef::wrap) {
			let class = parent.value().attr("class").unwrap_or("");
			if class.contains("il_ContainerListItem") || class.contains("il-std-item") {
				return parent;
			}
			link = parent;
		}
		link
	}

	fn collect_content_page_links(html: &Html) -> Vec<Result<Object>> {
		html.select(&CONTENT_PAGE_FILE_LINK)
			.filter_map(|link| {
				let href = link.value().attr("href")?;
				if Self::should_skip_link(href) || !href.to_ascii_lowercase().contains("file_id") {
					return None;
				}
				let mut name = link.text().collect::<String>();
				name = CONTENT_PAGE_SIZE_SUFFIX.replace_all(&name, "").trim().replace('\t', "").to_owned();
				let name = name.replace('/', "-");
				match URL::from_href(href).and_then(|url| Object::from_url(url, name, None)) {
					Ok(obj) => Some(Ok(obj)),
					Err(_) => None,
				}
			})
			.collect()
	}

	fn link_to_object(link: ElementRef) -> Result<Object> {
		let href = link.value().attr("href").context("link missing href")?;
		if Self::should_skip_link(href) {
			return Err(anyhow!("skipped link"));
		}
		let parent = Self::find_list_item_parent(link);
		Object::from_link(parent, link)
	}

	fn collect_title_links(scope: ElementRef) -> Vec<Result<Object>> {
		scope
			.select(&CONTAINER_ITEM_TITLE)
			.filter_map(|link| match Self::link_to_object(link) {
				Ok(obj) => Some(Ok(obj)),
				Err(_) => None,
			})
			.collect()
	}

	fn collect_card_links(html: &Html) -> Vec<Result<Object>> {
		html.select(&CARD_TITLE_LINK)
			.filter_map(|link| match Self::link_to_object(link) {
				Ok(obj) => Some(Ok(obj)),
				Err(_) => None,
			})
			.collect()
	}

	pub fn get_items(html: &Html, page_url: &str) -> Vec<Result<Object>> {
		let page_url_lower = page_url.to_ascii_lowercase();

		// ILIAS 9 personal desktop favourites
		if page_url_lower.contains("baseclass=ildashboardgui") {
			if let Some(block) = html.select(&BLOCK_DASH_FAV).next() {
				let items = Self::collect_title_links(block);
				if !items.is_empty() {
					return items;
				}
			}
			if let Some(favorites) = html.select(&BLOCK_FAVORITES).next() {
				let items = Self::collect_title_links(favorites);
				if !items.is_empty() {
					return items;
				}
			}
		}

		// membership overview lists all courses
		if page_url_lower.contains("ilmembershipoverviewgui") {
			let scope = html
				.select(&IL_CONTENT_CONTAINER)
				.next()
				.unwrap_or_else(|| html.root_element());
			let mut items = Self::collect_title_links(scope);
			items.extend(Self::collect_card_links(html));
			if !items.is_empty() {
				return items;
			}
		}

		let scope = html
			.select(&IL_CONTENT_CONTAINER)
			.next()
			.unwrap_or_else(|| html.root_element());

		let mut items = Self::collect_title_links(scope);
		items.extend(Self::collect_card_links(html));
		items.extend(Self::collect_content_page_links(html));

		if !items.is_empty() {
			return items;
		}

		// ILIAS 8 fallback
		let legacy_scope = if let Some(favorites) = html.select(&BLOCK_FAVORITES).next() {
			favorites
		} else {
			html.root_element()
		};
		legacy_scope
			.select(&CONTAINER_ITEMS)
			.flat_map(|item| {
				item.select(&CONTAINER_ITEM_TITLE)
					.next()
					.and_then(|link| match Self::link_to_object(link) {
						Ok(obj) => Some(Ok(obj)),
						Err(_) => None,
					})
			})
			.collect()
	}

	fn content_tab_url(html: &Html, _page_url: &str) -> Option<String> {
		let tab = html.select(&TAB_VIEW_CONTENT).next()?;
		if tab.value().attr("class").unwrap_or("").contains("active") {
			return None;
		}
		let link = tab.select(&LINKS).next()?;
		let href = link.value().attr("href")?;
		if href.starts_with("http://") || href.starts_with("https://") {
			Some(href.to_owned())
		} else if href.starts_with('/') {
			Some(format!("{}{}", ILIAS_URL.trim_end_matches('/'), href))
		} else {
			Some(format!("{}{}", ILIAS_URL, href))
		}
	}

	async fn resolve_page_url(&self, start_url: &str) -> Result<String> {
		let html = self.get_html(start_url).await?;
		if let Some(content_url) = Self::content_tab_url(&html, start_url) {
			log!(1, "Selecting content tab for {}", start_url);
			Ok(content_url)
		} else {
			Ok(start_url.to_owned())
		}
	}

	/// Returns subfolders, the main text in a course/folder/personal desktop and all links on the page.
	pub async fn get_course_content(&self, url: &URL) -> Result<(Vec<Result<Object>>, Option<String>, Vec<String>)> {
		let page_url = self.resolve_page_url(&url.url).await?;
		let (items, main_text, links, debug_save) = {
			let html = self.get_html(&page_url).await?;

			let main_text = if let Some(el) = html.select(&IL_CONTENT_CONTAINER).next() {
				if let Some(el) = el.select(&BLOCK_FAVORITES).next().or_else(|| el.select(&BLOCK_DASH_FAV).next()) {
					Some(wrap_html(&el.inner_html()))
				} else {
					Some(wrap_html(&el.inner_html()))
				}
			} else {
				None
			};
			let items = ILIAS::get_items(&html, &page_url);
			let links: Vec<String> = html
				.select(&LINKS)
				.flat_map(|x| x.value().attr("href").map(|x| x.to_owned()))
				.collect();
			let debug_save = if self.opt.debug_html && items.iter().all(|item| item.is_err()) {
				let slug = page_url
					.rsplit('/')
					.next()
					.unwrap_or("page")
					.chars()
					.take(80)
					.collect::<String>();
				Some((slug, html.html()))
			} else {
				None
			};
			(items, main_text, links, debug_save)
		};
		if let Some((slug, html_text)) = debug_save {
			if let Err(e) = save_debug_html(&self.opt.output, &slug, &html_text).await {
				warning!(e);
			}
		}
		Ok((items, main_text, links))
	}

	pub async fn get_course_content_tree(&self, ref_id: &str, cmd_node: &str) -> Result<Vec<Object>> {
		// TODO: this magically does not return sub-folders
		// opening the same url in browser does show sub-folders?!
		let url = format!(
			"{}ilias.php?ref_id={}&cmdClass=ilobjcoursegui&cmd=showRepTree&cmdNode={}&baseClass=ilRepositoryGUI&cmdMode=asynch&exp_cmd=getNodeAsync&node_id=exp_node_rep_exp_{}&exp_cont=il_expl2_jstree_cont_rep_exp&searchterm=",
			ILIAS_URL, ref_id, cmd_node, ref_id
		);
		let html = self.get_html_fragment(&url).await?;
		let mut items = Vec::new();
		for link in html.select(&LINKS) {
			if link.value().attr("href").is_some() {
				items.push(Object::from_link(link, link)?);
			} // else: disabled course
		}
		Ok(items)
	}
}

#[derive(Debug)]
pub enum Object {
	Course { name: String, url: URL },
	Folder { name: String, url: URL },
	Dashboard { url: URL },
	File { name: String, url: URL },
	Forum { name: String, url: URL },
	Thread { url: URL },
	Wiki { name: String, url: URL },
	ExerciseHandler { name: String, url: URL },
	Weblink { name: String, url: URL },
	Survey { name: String, url: URL },
	Presentation { name: String, url: URL },
	PluginDispatch { name: String, url: URL },
	Video { url: URL },
	Generic { name: String, url: URL },
}

use Object::*;

impl Object {
	pub fn name(&self) -> &str {
		match self {
			Course { name, .. }
			| Folder { name, .. }
			| File { name, .. }
			| Forum { name, .. }
			| Wiki { name, .. }
			| Weblink { name, .. }
			| Survey { name, .. }
			| Presentation { name, .. }
			| ExerciseHandler { name, .. }
			| PluginDispatch { name, .. }
			| Generic { name, .. } => name,
			Thread { url } => url.thr_pk.as_ref().unwrap(),
			Video { url } => &url.url,
			Dashboard { url } => &url.url,
		}
	}

	pub fn url(&self) -> &URL {
		match self {
			Course { url, .. }
			| Folder { url, .. }
			| Dashboard { url }
			| File { url, .. }
			| Forum { url, .. }
			| Thread { url }
			| Wiki { url, .. }
			| Weblink { url, .. }
			| Survey { url, .. }
			| Presentation { url, .. }
			| ExerciseHandler { url, .. }
			| PluginDispatch { url, .. }
			| Video { url }
			| Generic { url, .. } => url,
		}
	}

	pub fn kind(&self) -> &str {
		match self {
			Course { .. } => "course",
			Folder { .. } => "folder",
			Dashboard { .. } => "dashboard",
			File { .. } => "file",
			Forum { .. } => "forum",
			Thread { .. } => "thread",
			Wiki { .. } => "wiki",
			Weblink { .. } => "weblink",
			Survey { .. } => "survey",
			Presentation { .. } => "presentation",
			ExerciseHandler { .. } => "exercise handler",
			PluginDispatch { .. } => "plugin dispatch",
			Video { .. } => "video",
			Generic { .. } => "generic",
		}
	}

	pub fn is_dir(&self) -> bool {
		matches!(
			self,
			Course { .. }
				| Folder { .. } | Dashboard { .. }
				| Forum { .. } | Thread { .. }
				| Wiki { .. } | ExerciseHandler { .. }
				| PluginDispatch { .. }
		)
	}

	pub fn from_link(item: ElementRef, link: ElementRef) -> Result<Self> {
		let name = link.text().collect::<String>().replace('/', "-").trim().to_owned();
		let url = URL::from_href(link.value().attr("href").context("link missing href")?)?;
		Object::from_url(url, name, Some(item))
	}

	fn file_name_from_item(mut name: String, item: Option<ElementRef>) -> String {
		if name.contains('.') {
			return name;
		}
		let Some(item_el) = item else {
			return name;
		};
		let ext_text = item_el
			.select(&ITEM_PROPERTIES)
			.next()
			.and_then(|props| props.select(&ITEM_PROP).next())
			.or_else(|| item_el.select(&ITEM_PROP).next())
			.map(|e| e.text().collect::<String>().trim().to_owned())
			.filter(|s| !s.is_empty() && !s.contains(':'));
		if let Some(ext) = ext_text {
			let ext = ext.to_ascii_lowercase();
			if ext.len() <= 6 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
				name = format!("{}.{}", name, ext);
			}
		}
		name
	}

	pub fn from_url(mut url: URL, mut name: String, item: Option<ElementRef>) -> Result<Self> {
		if url.thr_pk.is_some() {
			return Ok(Thread { url });
		}

		// Content page file links: ?file_id=...
		if url.url.to_ascii_lowercase().contains("file_id=") {
			if let Ok(parsed) = Url::parse(&url.url) {
				for (k, v) in parsed.query_pairs() {
					if k == "file_id" {
						let file_id = v.into_owned();
						url.url = format!("{}goto.php/file/{}/download", ILIAS_URL, file_id);
						url.ref_id = file_id;
						name = Self::file_name_from_item(name, item);
						return Ok(File { name, url });
					}
				}
			}
		}

		if url.url.contains("goto.php") {
			let target = url.target.as_deref().unwrap_or("NONE");
			if target.starts_with("wiki_") {
				return Ok(Wiki {
					name,
					url, // TODO: insert ref_id here
				});
			}
			if target.starts_with("root_") {
				// magazine link
				return Ok(Generic { name, url });
			}
			if target.starts_with("crs_") || target.starts_with("grp_") {
				let ref_id = url.target.as_ref().unwrap().split('_').nth(1).unwrap();
				url.ref_id = ref_id.to_owned();
				return Ok(Course { name, url });
			}
			if target.starts_with("frm_") {
				// TODO: extract post link? (this codepath should only be hit when parsing the content tree)
				let ref_id = url.target.as_ref().unwrap().split('_').nth(1).unwrap();
				url.ref_id = ref_id.to_owned();
				return Ok(Forum { name, url });
			}
			if target.starts_with("exc_") {
				let ref_id = url.target.as_ref().unwrap().split('_').nth(1).unwrap();
				url.ref_id = ref_id.to_owned();
				return Ok(ExerciseHandler { name, url });
			}
			if target.starts_with("lm_") {
				// fancy interactive task
				return Ok(Presentation { name, url });
			}
			if target.starts_with("fold_") || target.starts_with("copa_") {
				let ref_id = url.target.as_ref().unwrap().split('_').nth(1).unwrap();
				url.ref_id = ref_id.to_owned();
				return Ok(Folder { name, url });
			}
			if target.starts_with("file_") {
				if !target.ends_with("download") {
					let file_id = url.ref_id.clone();
					if !file_id.is_empty() {
						url.url = format!("{}goto.php/file/{}/download", ILIAS_URL, file_id);
					}
				}
				name = Self::file_name_from_item(name, item);
				return Ok(File { name, url });
			}
			if target != "NONE" {
				// ILIAS 9 path-style goto links with known ref_id
				if !url.ref_id.is_empty() {
					return Ok(Folder { name, url });
				}
			}
			return Ok(Generic { name, url });
		}

		if url.cmd.as_deref() == Some("showThreads") {
			return Ok(Forum { name, url });
		}

		// class name is *sometimes* in CamelCase
		Ok(match &*url.baseClass.to_ascii_lowercase() {
			"ilexercisehandlergui" => ExerciseHandler { name, url },
			"ililwikihandlergui" => Wiki { name, url },
			"illinkresourcehandlergui" => Weblink { name, url },
			"ilobjsurveygui" => Survey { name, url },
			"illmpresentationgui" => Presentation { name, url },
			"ilrepositorygui" => match url.cmd.as_deref() {
				Some("view") | Some("render") => Folder { name, url },
				Some("sendfile") => File {
					name: Self::file_name_from_item(name, item),
					url,
				},
				Some(_) => Generic { name, url },
				None => Course { name, url },
			},
			"ilobjplugindispatchgui" => PluginDispatch { name, url },
			// both the dashboard and the membership overview page work the same
			"ildashboardgui" | "ilmembershipoverviewgui" => Dashboard { url },
			_ => Generic { name, url },
		})
	}

	pub(crate) fn is_ignored_by_option(&self, opt: &Opt) -> bool {
		(matches!(self, Object::Forum { .. }) && !opt.forum)
			|| (matches!(self, Object::Video { .. }) && opt.no_videos)
			|| (matches!(self, Object::File { .. }) && opt.skip_files)
	}
}

#[allow(non_snake_case)]
#[derive(Debug)]
pub struct URL {
	pub url: String,
	baseClass: String,
	pub cmd: Option<String>,
	pub thr_pk: Option<String>,
	pub ref_id: String,
	target: Option<String>,
}

#[allow(non_snake_case)]
impl URL {
	pub fn raw(url: String) -> Self {
		URL {
			url,
			baseClass: String::new(),
			cmd: None,
			thr_pk: None,
			ref_id: String::new(),
			target: None,
		}
	}

	pub fn from_href(href: &str) -> Result<Self> {
		let url = if !href.starts_with(ILIAS_URL) {
			Url::parse(&format!("{}{}", ILIAS_URL, href))?
		} else {
			Url::parse(href)?
		};
		let mut baseClass = String::new();
		let mut cmd = None;
		let mut thr_pk = None;
		let mut ref_id = String::new();
		let mut target = None;
		for (k, v) in url.query_pairs() {
			match &*k {
				"baseClass" => baseClass = v.into_owned(),
				"cmd" => cmd = Some(v.into_owned()),
				"thr_pk" => thr_pk = Some(v.into_owned()),
				"ref_id" => ref_id = v.into_owned(),
				"target" => target = Some(v.into_owned()),
				_ => {},
			}
		}
		let url_string: String = url.clone().into();
		if let Some(cap) = GOTO_PATH.captures(&url_string) {
			let typ = cap.get(1).unwrap().as_str();
			let id = cap.get(2).unwrap().as_str();
			target = Some(format!("{}_{}", typ, id));
			if ref_id.is_empty() {
				ref_id = id.to_owned();
			}
		}
		if let Some(cap) = GOTO_FILE_DOWNLOAD.captures(&url_string) {
			let id = cap.get(1).unwrap().as_str();
			target = Some(format!("file_{}_download", id));
			if ref_id.is_empty() {
				ref_id = id.to_owned();
			}
		}
		Ok(URL {
			url: url_string,
			baseClass,
			cmd,
			thr_pk,
			ref_id,
			target,
		})
	}
}
