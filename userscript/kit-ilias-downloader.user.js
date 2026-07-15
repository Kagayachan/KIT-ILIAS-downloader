// ==UserScript==
// @name         KIT ILIAS bulk downloader
// @namespace    https://github.com/kagayachan/KIT-ILIAS-downloader
// @version      0.1.0
// @description  Bulk download files from KIT ILIAS (ILIAS 9) as a single ZIP. Works on desktop and mobile (Android Firefox + Tampermonkey/Violentmonkey, iOS/iPadOS Safari + Userscripts).
// @author       KIT-ILIAS-downloader contributors
// @license      GPL-3.0-or-later
// @match        https://ilias.studium.kit.edu/*
// @grant        GM_xmlhttpRequest
// @grant        GM.xmlHttpRequest
// @connect      *
// @require      https://unpkg.com/fflate@0.8.2/umd/index.js
// @run-at       document-idle
// @noframes
// ==/UserScript==

/*
 * JS port of the crawling logic of the Rust CLI in this repository (src/ilias.rs and friends).
 * Login/Shibboleth is NOT ported: the script reuses the browser's logged-in ILIAS session cookies.
 */

(function () {
	'use strict';

	const ILIAS_URL = 'https://ilias.studium.kit.edu/';
	const DEFAULT_DESKTOP_URL = ILIAS_URL + 'ilias.php?baseClass=ilDashboardGUI&cmd=show';
	const DEFAULT_ALL_COURSES_URL = ILIAS_URL + 'ilias.php?cmdClass=ilmembershipoverviewgui&baseClass=ilmembershipoverviewgui';

	// ---------------------------------------------------------------------
	// Selectors / regexes (ported from src/ilias.rs)
	// ---------------------------------------------------------------------
	const SEL = {
		LINKS: 'a',
		ALERT_DANGER: 'div.alert-danger, .il_ItemAlertProperty',
		IL_CONTENT_CONTAINER: '#il_center_col, #ilContentContainer',
		BLOCK_FAVORITES: '#block_pditems_0',
		BLOCK_DASH_FAV: '#block_dash_fav_0',
		ITEM_PROP: 'span.il_ItemProperty',
		ITEM_PROPERTIES: '.il_ItemProperties',
		CONTAINER_ITEMS: 'div.il_ContainerListItem, .il-std-item',
		CONTAINER_ITEM_TITLE: 'a.il_ContainerItemTitle, .il-item-title > a, .il-item-title a',
		CARD_TITLE_LINK: '.card-title a',
		CONTENT_PAGE_FILE_LINK: 'a.ilc_flist_a_FileListItemLink',
		TAB_VIEW_CONTENT: '#tab_view_content',
	};
	const CONTENT_PAGE_SIZE_SUFFIX = /\([\d,.]+ [MK]B\)/g;
	const GOTO_PATH = /goto\.php\/([a-z]+)\/(\d+)/;
	const GOTO_FILE_DOWNLOAD = /goto\.php\/file\/(\d+)\/download/;
	const EXPAND_LINK = /expand=\d/;
	// Opencast (ported from src/ilias/video.rs and src/ilias/plugin_dispatch.rs)
	const XOCT_REGEX = /il\.Opencast\.Paella\.player\.init\(\s+([\s\S]+),\s/m;
	const XOCT_STREAMS_REGEX = /(\{"streams"[\s\S]+?),\s*\{"paella_config_file/;
	const XOCT_LIST_URL = /ilias\.php\?baseClass=ilobjplugindispatchgui&cmdNode=.{9}&cmdClass=xoctEventGUI&ref_id=\d+&async=true/;

	// ---------------------------------------------------------------------
	// Small helpers
	// ---------------------------------------------------------------------
	// ported from src/util.rs (Windows-safe superset so extracted ZIPs work everywhere)
	function fileEscape(s) {
		return s.replace(/[/\\:<>"|?*]/g, '-');
	}

	function absolutizeUrl(href) {
		if (href.startsWith('http://') || href.startsWith('https://')) return href;
		if (href.startsWith('ilias.studium.kit.edu')) return 'https://' + href;
		return new URL(href, ILIAS_URL).href;
	}

	function sleep(ms) {
		return new Promise((resolve) => setTimeout(resolve, ms));
	}

	// ---------------------------------------------------------------------
	// URL parsing (ported from URL::from_href, src/ilias.rs)
	// ---------------------------------------------------------------------
	function parseIliasUrl(href) {
		const url = new URL(href, ILIAS_URL);
		const q = url.searchParams;
		let baseClass = q.get('baseClass') || '';
		const cmd = q.get('cmd');
		const thrPk = q.get('thr_pk');
		let refId = q.get('ref_id') || '';
		let target = q.get('target');
		const urlString = url.href;
		let m = GOTO_PATH.exec(urlString);
		if (m) {
			target = m[1] + '_' + m[2];
			if (!refId) refId = m[2];
		}
		m = GOTO_FILE_DOWNLOAD.exec(urlString);
		if (m) {
			target = 'file_' + m[1] + '_download';
			if (!refId) refId = m[1];
		}
		return { url: urlString, baseClass, cmd, thrPk, refId, target };
	}

	function rawUrl(href) {
		return { url: absolutizeUrl(href), baseClass: '', cmd: null, thrPk: null, refId: '', target: null };
	}

	// ---------------------------------------------------------------------
	// Object type detection (ported from Object::from_url, src/ilias.rs)
	// ---------------------------------------------------------------------
	// item: the surrounding list item element (or null), used to guess file extensions
	function fileNameFromItem(name, item) {
		if (name.includes('.')) return name;
		if (!item) return name;
		let propEl = null;
		const props = item.querySelector(SEL.ITEM_PROPERTIES);
		if (props) propEl = props.querySelector(SEL.ITEM_PROP);
		if (!propEl) propEl = item.querySelector(SEL.ITEM_PROP);
		if (!propEl) return name;
		const extText = (propEl.textContent || '').trim();
		if (!extText || extText.includes(':')) return name;
		const ext = extText.toLowerCase();
		if (ext.length <= 6 && /^[a-z0-9]+$/.test(ext)) {
			return name + '.' + ext;
		}
		return name;
	}

	function objectFromUrl(url, name, item) {
		if (url.thrPk) return { kind: 'thread', name, url };

		// content page file links: ?file_id=...
		if (url.url.toLowerCase().includes('file_id=')) {
			const parsed = new URL(url.url);
			const fileId = parsed.searchParams.get('file_id');
			if (fileId) {
				url.url = ILIAS_URL + 'goto.php/file/' + fileId + '/download';
				url.refId = fileId;
				return { kind: 'file', name: fileNameFromItem(name, item), url };
			}
		}

		if (url.url.includes('goto.php')) {
			const target = url.target || 'NONE';
			if (target.startsWith('wiki_')) return { kind: 'wiki', name, url };
			if (target.startsWith('root_')) return { kind: 'generic', name, url };
			if (target.startsWith('crs_') || target.startsWith('grp_')) {
				url.refId = target.split('_')[1];
				return { kind: 'course', name, url };
			}
			if (target.startsWith('frm_')) {
				url.refId = target.split('_')[1];
				return { kind: 'forum', name, url };
			}
			if (target.startsWith('exc_')) {
				url.refId = target.split('_')[1];
				return { kind: 'exercise', name, url };
			}
			if (target.startsWith('lm_')) return { kind: 'presentation', name, url };
			if (target.startsWith('fold_') || target.startsWith('copa_')) {
				url.refId = target.split('_')[1];
				return { kind: 'folder', name, url };
			}
			if (target.startsWith('file_')) {
				if (!target.endsWith('download')) {
					if (url.refId) {
						url.url = ILIAS_URL + 'goto.php/file/' + url.refId + '/download';
					}
				}
				return { kind: 'file', name: fileNameFromItem(name, item), url };
			}
			if (target !== 'NONE' && url.refId) {
				// ILIAS 9 path-style goto links with known ref_id
				return { kind: 'folder', name, url };
			}
			return { kind: 'generic', name, url };
		}

		if (url.cmd === 'showThreads') return { kind: 'forum', name, url };

		switch (url.baseClass.toLowerCase()) {
			case 'ilexercisehandlergui':
				return { kind: 'exercise', name, url };
			case 'ililwikihandlergui':
				return { kind: 'wiki', name, url };
			case 'illinkresourcehandlergui':
				return { kind: 'weblink', name, url };
			case 'ilobjsurveygui':
				return { kind: 'survey', name, url };
			case 'illmpresentationgui':
				return { kind: 'presentation', name, url };
			case 'ilrepositorygui':
				if (url.cmd === 'view' || url.cmd === 'render') return { kind: 'folder', name, url };
				if (url.cmd === 'sendfile') return { kind: 'file', name: fileNameFromItem(name, item), url };
				if (url.cmd) return { kind: 'generic', name, url };
				return { kind: 'course', name, url };
			case 'ilobjplugindispatchgui':
				return { kind: 'pluginDispatch', name, url };
			case 'ildashboardgui':
			case 'ilmembershipoverviewgui':
				return { kind: 'dashboard', name, url };
			default:
				return { kind: 'generic', name, url };
		}
	}

	function isContainer(obj) {
		return obj.kind === 'course' || obj.kind === 'folder' || obj.kind === 'dashboard';
	}

	// ---------------------------------------------------------------------
	// Page item extraction (ported from ILIAS::get_items, src/ilias.rs)
	// ---------------------------------------------------------------------
	function shouldSkipLink(href) {
		const h = href.toLowerCase();
		return (
			h.includes('ilmailgui') ||
			(h.includes('cmd=manage') && h.includes('ilpdselecteditemsblockgui')) ||
			h.includes('cmd=jumptomemberships') ||
			h.includes('block_type=pditems')
		);
	}

	function findListItemParent(link) {
		let el = link;
		while (el.parentElement) {
			const parent = el.parentElement;
			const cls = parent.getAttribute && (parent.getAttribute('class') || '');
			if (cls && (cls.includes('il_ContainerListItem') || cls.includes('il-std-item'))) {
				return parent;
			}
			el = parent;
		}
		return link;
	}

	function linkToObject(link) {
		const href = link.getAttribute('href');
		if (!href || shouldSkipLink(href)) return null;
		const parent = findListItemParent(link);
		const name = (link.textContent || '').replace(/\//g, '-').trim();
		try {
			return objectFromUrl(parseIliasUrl(href), name, parent);
		} catch (e) {
			return null;
		}
	}

	function collectTitleLinks(scope) {
		const out = [];
		for (const link of scope.querySelectorAll(SEL.CONTAINER_ITEM_TITLE)) {
			const obj = linkToObject(link);
			if (obj) out.push(obj);
		}
		return out;
	}

	function collectCardLinks(doc) {
		const out = [];
		for (const link of doc.querySelectorAll(SEL.CARD_TITLE_LINK)) {
			const obj = linkToObject(link);
			if (obj) out.push(obj);
		}
		return out;
	}

	function collectContentPageLinks(doc) {
		const out = [];
		for (const link of doc.querySelectorAll(SEL.CONTENT_PAGE_FILE_LINK)) {
			const href = link.getAttribute('href');
			if (!href || shouldSkipLink(href) || !href.toLowerCase().includes('file_id')) continue;
			let name = link.textContent || '';
			name = name.replace(CONTENT_PAGE_SIZE_SUFFIX, '').trim().replace(/\t/g, '').replace(/\//g, '-');
			try {
				out.push(objectFromUrl(parseIliasUrl(href), name, null));
			} catch (e) {
				/* ignore */
			}
		}
		return out;
	}

	function getItems(doc, pageUrl) {
		const pageUrlLower = pageUrl.toLowerCase();

		// ILIAS 9 personal desktop favourites
		if (pageUrlLower.includes('baseclass=ildashboardgui')) {
			const dashFav = doc.querySelector(SEL.BLOCK_DASH_FAV);
			if (dashFav) {
				const items = collectTitleLinks(dashFav);
				if (items.length) return items;
			}
			const favorites = doc.querySelector(SEL.BLOCK_FAVORITES);
			if (favorites) {
				const items = collectTitleLinks(favorites);
				if (items.length) return items;
			}
		}

		// membership overview lists all courses
		if (pageUrlLower.includes('ilmembershipoverviewgui')) {
			const scope = doc.querySelector(SEL.IL_CONTENT_CONTAINER) || doc;
			const items = collectTitleLinks(scope).concat(collectCardLinks(doc));
			if (items.length) return items;
		}

		const scope = doc.querySelector(SEL.IL_CONTENT_CONTAINER) || doc;
		let items = collectTitleLinks(scope).concat(collectCardLinks(doc), collectContentPageLinks(doc));
		if (items.length) return items;

		// ILIAS 8 fallback
		const legacyScope = doc.querySelector(SEL.BLOCK_FAVORITES) || doc;
		items = [];
		for (const item of legacyScope.querySelectorAll(SEL.CONTAINER_ITEMS)) {
			const link = item.querySelector(SEL.CONTAINER_ITEM_TITLE);
			if (!link) continue;
			const obj = linkToObject(link);
			if (obj) items.push(obj);
		}
		return items;
	}

	function contentTabUrl(doc) {
		const tab = doc.querySelector(SEL.TAB_VIEW_CONTENT);
		if (!tab) return null;
		if ((tab.getAttribute('class') || '').includes('active')) return null;
		const link = tab.querySelector(SEL.LINKS);
		if (!link) return null;
		const href = link.getAttribute('href');
		if (!href) return null;
		return absolutizeUrl(href);
	}

	function isErrorResponse(doc) {
		return !!doc.querySelector(SEL.ALERT_DANGER);
	}

	// ---------------------------------------------------------------------
	// File extension inference (ported from src/ilias/file.rs)
	// ---------------------------------------------------------------------
	const CONTENT_TYPE_EXT = {
		'application/pdf': 'pdf',
		'application/vnd.ms-powerpoint': 'ppt',
		'application/vnd.openxmlformats-officedocument.presentationml.presentation': 'pptx',
		'application/msword': 'doc',
		'application/vnd.openxmlformats-officedocument.wordprocessingml.document': 'docx',
		'application/vnd.ms-excel': 'xls',
		'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet': 'xlsx',
		'application/zip': 'zip',
		'image/jpeg': 'jpg',
		'image/png': 'png',
		'text/plain': 'txt',
	};

	function extensionFromContentType(contentType) {
		const mime = contentType.split(';')[0].trim().toLowerCase();
		return CONTENT_TYPE_EXT[mime] || null;
	}

	function extensionFromContentDisposition(value) {
		for (let part of value.split(';')) {
			part = part.trim();
			// RFC 5987 filename*=UTF-8''...
			if (part.toLowerCase().startsWith("filename*=utf-8''")) {
				try {
					const filename = decodeURIComponent(part.slice("filename*=utf-8''".length));
					const dot = filename.lastIndexOf('.');
					if (dot > 0) return filename.slice(dot + 1).toLowerCase();
				} catch (e) {
					/* ignore */
				}
			}
			if (part.toLowerCase().startsWith('filename=')) {
				const filename = part.slice('filename='.length).replace(/^"|"$/g, '');
				const dot = filename.lastIndexOf('.');
				if (dot > 0) return filename.slice(dot + 1).toLowerCase();
			}
		}
		return null;
	}

	function nameWithExtension(name, headers) {
		if (/\.[A-Za-z0-9]{1,8}$/.test(name)) return name;
		const cd = headers.get('content-disposition');
		if (cd) {
			const ext = extensionFromContentDisposition(cd);
			if (ext) return name + '.' + ext;
		}
		const ct = headers.get('content-type');
		if (ct) {
			const ext = extensionFromContentType(ct);
			if (ext) return name + '.' + ext;
		}
		return name;
	}

	// ---------------------------------------------------------------------
	// Networking: rate limiting + concurrency (ported from src/queue.rs)
	// ---------------------------------------------------------------------
	function createState(options) {
		return {
			options,
			aborted: false,
			abortController: typeof AbortController !== 'undefined' ? new AbortController() : null,
			nextSlot: 0,
			runningJobs: 0,
			jobWaiters: [],
			visited: new Set(),
			dirNames: new Map(), // dir path -> Set of used names
			zip: null,
			zipChunks: [],
			zipError: null,
			stats: { pages: 0, filesDone: 0, filesTotal: 0, bytes: 0, skipped: [] },
			errors: [],
			onProgress: () => {},
		};
	}

	async function rateTicket(state) {
		const interval = 60000 / state.options.rate;
		const now = Date.now();
		const slot = Math.max(now, state.nextSlot);
		state.nextSlot = slot + interval;
		if (slot > now) await sleep(slot - now);
		if (state.aborted) throw new Error('aborted');
	}

	async function acquireJob(state) {
		while (state.runningJobs >= state.options.jobs) {
			await new Promise((resolve) => state.jobWaiters.push(resolve));
		}
		state.runningJobs++;
	}

	function releaseJob(state) {
		state.runningJobs--;
		const next = state.jobWaiters.shift();
		if (next) next();
	}

	async function fetchRaw(state, url) {
		await rateTicket(state);
		const resp = await fetch(url, {
			credentials: 'include',
			signal: state.abortController ? state.abortController.signal : undefined,
		});
		const respUrl = resp.url || '';
		if (respUrl.includes('reloadpublic=1') || respUrl.includes('cmd=force_login')) {
			throw new Error('not logged in / session expired');
		}
		if (!resp.ok) {
			throw new Error('HTTP ' + resp.status + ' for ' + url);
		}
		return resp;
	}

	async function fetchDocument(state, url) {
		const resp = await fetchRaw(state, url);
		const text = await resp.text();
		const doc = new DOMParser().parseFromString(text, 'text/html');
		state.stats.pages++;
		state.onProgress();
		return { doc, text, finalUrl: resp.url || url };
	}

	async function fetchHtmlChecked(state, url) {
		const { doc, text, finalUrl } = await fetchDocument(state, url);
		if (isErrorResponse(doc)) throw new Error('ILIAS error page at ' + url);
		return { doc, text, finalUrl };
	}

	// Cross-origin fallback (Opencast video servers) via GM_xmlhttpRequest
	function gmFetchBinary(url) {
		const gmXhr =
			(typeof GM_xmlhttpRequest !== 'undefined' && GM_xmlhttpRequest) ||
			(typeof GM !== 'undefined' && GM.xmlHttpRequest);
		if (!gmXhr) return Promise.reject(new Error('GM_xmlhttpRequest unavailable for cross-origin download'));
		return new Promise((resolve, reject) => {
			gmXhr({
				method: 'GET',
				url,
				responseType: 'arraybuffer',
				onload: (r) => {
					if (r.status >= 200 && r.status < 300) resolve(new Uint8Array(r.response));
					else reject(new Error('HTTP ' + r.status + ' for ' + url));
				},
				onerror: () => reject(new Error('network error for ' + url)),
			});
		});
	}

	async function fetchBinary(state, url) {
		try {
			const resp = await fetchRaw(state, url);
			const buf = await resp.arrayBuffer();
			return { data: new Uint8Array(buf), headers: resp.headers };
		} catch (e) {
			if (state.aborted) throw e;
			// cross-origin (e.g. Opencast CDN): retry via GM_xmlhttpRequest
			if (!url.startsWith(ILIAS_URL)) {
				const data = await gmFetchBinary(url);
				return { data, headers: new Headers() };
			}
			throw e;
		}
	}

	// ---------------------------------------------------------------------
	// ZIP output (fflate)
	// ---------------------------------------------------------------------
	function zipInit(state) {
		state.zipChunks = [];
		state.zip = new fflate.Zip((err, chunk, final) => {
			if (err) {
				state.zipError = err;
				return;
			}
			state.zipChunks.push(chunk);
		});
	}

	function uniquePath(state, dir, name) {
		if (!state.dirNames.has(dir)) state.dirNames.set(dir, new Set());
		const used = state.dirNames.get(dir);
		let candidate = name;
		if (used.has(candidate)) {
			const dot = name.lastIndexOf('.');
			const stem = dot > 0 ? name.slice(0, dot) : name;
			const ext = dot > 0 ? name.slice(dot) : '';
			for (let i = 2; ; i++) {
				candidate = stem + ' (' + i + ')' + ext;
				if (!used.has(candidate)) break;
			}
		}
		used.add(candidate);
		return dir ? dir + '/' + candidate : candidate;
	}

	function zipAddFile(state, path, data) {
		if (state.zipError) throw state.zipError;
		const entry = new fflate.ZipPassThrough(path);
		state.zip.add(entry);
		entry.push(data, true);
		state.stats.bytes += data.length;
	}

	function zipFinish(state) {
		return new Promise((resolve, reject) => {
			if (state.zipError) return reject(state.zipError);
			// replace the callback's final handling: end() flushes synchronously for PassThrough entries
			state.zip.ondata = (err, chunk, final) => {
				if (err) return reject(err);
				state.zipChunks.push(chunk);
				if (final) resolve(new Blob(state.zipChunks, { type: 'application/zip' }));
			};
			state.zip.end();
		});
	}

	function triggerDownload(blob, filename) {
		const a = document.createElement('a');
		const url = URL.createObjectURL(blob);
		a.href = url;
		a.download = filename;
		document.body.appendChild(a);
		a.click();
		setTimeout(() => {
			URL.revokeObjectURL(url);
			a.remove();
		}, 60000);
	}

	// ---------------------------------------------------------------------
	// Crawling (ported from main.rs process() + course.rs/folder.rs/file.rs)
	// ---------------------------------------------------------------------
	async function resolvePage(state, startUrl) {
		const first = await fetchHtmlChecked(state, startUrl);
		const tabUrl = contentTabUrl(first.doc);
		if (tabUrl && tabUrl !== startUrl) {
			return await fetchHtmlChecked(state, tabUrl);
		}
		return first;
	}

	async function getCourseContent(state, url) {
		const { doc, finalUrl } = await resolvePage(state, url.url);
		const items = getItems(doc, finalUrl);
		const links = [];
		for (const a of doc.querySelectorAll(SEL.LINKS)) {
			const href = a.getAttribute('href');
			if (href) links.push(href);
		}
		return { items, links };
	}

	async function downloadFile(state, path, obj) {
		state.stats.filesTotal++;
		state.onProgress();
		await acquireJob(state);
		try {
			const { data, headers } = await fetchBinary(state, obj.url.url);
			const dir = path.slice(0, path.lastIndexOf('/') === -1 ? 0 : path.lastIndexOf('/'));
			let base = path.slice(dir ? dir.length + 1 : 0);
			base = nameWithExtension(base, headers);
			const finalPath = uniquePath(state, dir, base);
			zipAddFile(state, finalPath, data);
			state.stats.filesDone++;
			state.onProgress(finalPath);
		} finally {
			releaseJob(state);
		}
	}

	// ported from src/ilias/video.rs
	function parseOpencastJson(html) {
		let m = XOCT_STREAMS_REGEX.exec(html);
		if (m) return JSON.parse(m[1].trim());
		m = XOCT_REGEX.exec(html);
		if (!m) throw new Error('xoct player json not found');
		const json = m[1].split(',\n')[0];
		return JSON.parse(json.trim());
	}

	async function downloadVideo(state, path, obj) {
		const resp = await fetchRaw(state, absolutizeUrl(obj.url.url));
		const html = await resp.text();
		const json = parseOpencastJson(html);
		const streams = json.streams;
		if (!Array.isArray(streams)) throw new Error('video streams not found');
		const tasks = [];
		if (streams.length === 1) {
			const src = streams[0] && streams[0].sources && streams[0].sources.mp4 && streams[0].sources.mp4[0] && streams[0].sources.mp4[0].src;
			if (!src) throw new Error('video src not found');
			tasks.push(downloadFile(state, path, { kind: 'file', name: path, url: rawUrl(src) }));
		} else {
			for (let i = 0; i < streams.length; i++) {
				const s = streams[i];
				const src = s && s.sources && s.sources.mp4 && s.sources.mp4[0] && s.sources.mp4[0].src;
				if (!src) continue;
				const streamPath = path.replace(/\.mp4$/, '') + '/Stream' + (i + 1) + '.mp4';
				tasks.push(downloadFile(state, streamPath, { kind: 'file', name: streamPath, url: rawUrl(src) }));
			}
		}
		await Promise.all(tasks);
	}

	// ported from src/ilias/plugin_dispatch.rs (Opencast lecture list)
	async function downloadPluginDispatch(state, path, obj) {
		if (!state.options.includeVideos) return;
		const first = await fetchRaw(state, absolutizeUrl(obj.url.url));
		const firstHtml = await first.text();
		const listUrlMatch = XOCT_LIST_URL.exec(firstHtml);
		if (!listUrlMatch) throw new Error('failed to find xoct event link');
		const listResp = await fetchRaw(state, ILIAS_URL + listUrlMatch[0]);
		const listHtml = await listResp.text();
		const listDoc = new DOMParser().parseFromString(listHtml, 'text/html');
		let fullUrlHref = null;
		for (const a of listDoc.querySelectorAll('a')) {
			const href = a.getAttribute('href');
			if (href && href.includes('trows=800')) {
				fullUrlHref = href;
				break;
			}
		}
		if (!fullUrlHref) throw new Error('video list link not found');
		const fullUrl = new URL(absolutizeUrl(fullUrlHref));
		const params = fullUrl.searchParams;
		if (params.has('cmd')) params.set('cmd', 'asyncGetTableGUI');
		if (params.has('cmdClass')) params.set('cmdClass', 'xocteventgui');
		params.set('cmdMode', 'asynch');
		const tableResp = await fetchRaw(state, fullUrl.href);
		const tableHtml = await tableResp.text();
		const tableDoc = new DOMParser().parseFromString(tableHtml, 'text/html');
		const tasks = [];
		for (const row of tableDoc.querySelectorAll('.ilTableOuter > div > table > tbody > tr, table tbody tr')) {
			const link = row.querySelector('a[target="_blank"]');
			if (!link) continue;
			const cells = row.querySelectorAll('td');
			if (cells.length < 3) continue;
			const title = (cells[2].textContent || '').trim();
			if (!title || title.startsWith('<div')) continue;
			const href = link.getAttribute('href');
			if (!href) continue;
			const videoPath = path + '/' + fileEscape(title) + '.mp4';
			tasks.push(
				processObject(state, videoPath, { kind: 'video', name: title, url: rawUrl(href) })
			);
		}
		await Promise.all(tasks);
	}

	async function downloadContainer(state, path, obj) {
		const content = await getCourseContent(state, obj.url);

		// expand all sessions (folder.rs)
		if (obj.kind === 'folder' || obj.kind === 'dashboard') {
			for (const href of content.links) {
				if (EXPAND_LINK.test(href)) {
					const expandUrl = parseIliasUrl(href);
					if (!state.visited.has(expandUrl.url)) {
						state.visited.add(expandUrl.url);
						return await downloadContainer(state, path, { kind: obj.kind, name: obj.name, url: expandUrl });
					}
				}
			}
		}

		const tasks = [];
		for (const item of content.items) {
			const childPath = path ? path + '/' + fileEscape(item.name) : fileEscape(item.name);
			tasks.push(processObject(state, childPath, item));
		}
		await Promise.all(tasks);
	}

	async function processObject(state, path, obj) {
		if (state.aborted) return;
		const visitKey = obj.url.url;
		if (isContainer(obj) || obj.kind === 'pluginDispatch') {
			if (state.visited.has(visitKey)) return;
			state.visited.add(visitKey);
		}
		try {
			switch (obj.kind) {
				case 'course':
				case 'folder':
				case 'dashboard':
					await downloadContainer(state, path, obj);
					break;
				case 'file':
					await downloadFile(state, path, obj);
					break;
				case 'pluginDispatch':
					await downloadPluginDispatch(state, path, obj);
					break;
				case 'video':
					if (state.options.includeVideos) await downloadVideo(state, path, obj);
					else state.stats.skipped.push('[video] ' + path);
					break;
				default:
					state.stats.skipped.push('[' + obj.kind + '] ' + path);
					break;
			}
		} catch (e) {
			if (state.aborted) return;
			state.errors.push(path + ': ' + (e && e.message ? e.message : e));
			state.onProgress();
		}
	}

	// ---------------------------------------------------------------------
	// Top-level runner
	// ---------------------------------------------------------------------
	function rootObjectForScope(scope) {
		if (scope === 'all') {
			return { obj: objectFromUrl(parseIliasUrl(DEFAULT_ALL_COURSES_URL), 'ILIAS', null), rootName: 'ILIAS' };
		}
		if (scope === 'desktop') {
			return { obj: objectFromUrl(parseIliasUrl(DEFAULT_DESKTOP_URL), 'Favourites', null), rootName: 'ILIAS-Favourites' };
		}
		// current page
		const href = window.location.href;
		const obj = objectFromUrl(parseIliasUrl(href), document.title.replace(/\s*[–-]\s*ILIAS.*$/, '').trim() || 'page', null);
		if (!isContainer(obj)) {
			// force treating the current page as a folder listing
			obj.kind = 'folder';
		}
		const rootName = fileEscape(obj.name || 'ilias-page');
		return { obj, rootName };
	}

	async function runDownload(options, ui) {
		const state = createState(options);
		state.onProgress = ui.updateProgress.bind(ui, state);
		zipInit(state);
		const { obj, rootName } = rootObjectForScope(options.scope);
		ui.setRunning(state);
		try {
			await processObject(state, '', obj);
			if (state.aborted) {
				ui.setDone(state, null);
				return state;
			}
			const blob = await zipFinish(state);
			const stamp = new Date().toISOString().slice(0, 16).replace(/[T:]/g, '-');
			const filename = fileEscape(rootName) + '-' + stamp + '.zip';
			if (state.stats.filesDone > 0) {
				triggerDownload(blob, filename);
			}
			ui.setDone(state, filename);
		} catch (e) {
			state.errors.push('fatal: ' + (e && e.message ? e.message : e));
			ui.setDone(state, null);
		}
		return state;
	}

	// ---------------------------------------------------------------------
	// UI panel
	// ---------------------------------------------------------------------
	const PANEL_CSS = `
#kid-panel {
	position: fixed; right: 12px; bottom: 12px; z-index: 99999;
	font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
	font-size: 14px; color: #1a1a1a;
}
#kid-toggle {
	background: #007a5e; color: #fff; border: none; border-radius: 24px;
	padding: 12px 18px; font-size: 15px; font-weight: 600; cursor: pointer;
	box-shadow: 0 2px 8px rgba(0,0,0,.3);
}
#kid-body {
	display: none; background: #fff; border: 1px solid #ccc; border-radius: 12px;
	box-shadow: 0 4px 16px rgba(0,0,0,.25); padding: 14px; width: 300px;
	max-height: 70vh; overflow-y: auto; margin-bottom: 8px;
}
#kid-body.kid-open { display: block; }
#kid-body h3 { margin: 0 0 10px; font-size: 15px; }
#kid-body label { display: block; margin: 8px 0 2px; font-weight: 600; }
#kid-body select, #kid-body input[type=number] {
	width: 100%; padding: 6px; border: 1px solid #bbb; border-radius: 6px; box-sizing: border-box;
}
#kid-body .kid-check { display: flex; align-items: center; gap: 6px; margin-top: 8px; font-weight: 400; }
#kid-body .kid-check input { width: auto; }
#kid-start {
	width: 100%; margin-top: 12px; background: #007a5e; color: #fff; border: none;
	border-radius: 8px; padding: 10px; font-size: 15px; font-weight: 600; cursor: pointer;
}
#kid-start[disabled] { background: #999; cursor: default; }
#kid-cancel {
	width: 100%; margin-top: 6px; background: #b3261e; color: #fff; border: none;
	border-radius: 8px; padding: 8px; font-size: 14px; cursor: pointer; display: none;
}
#kid-status { margin-top: 10px; white-space: pre-wrap; word-break: break-word; font-size: 13px; color: #333; }
#kid-errors { margin-top: 6px; color: #b3261e; font-size: 12px; white-space: pre-wrap; word-break: break-word; max-height: 120px; overflow-y: auto; }
#kid-note { margin-top: 8px; font-size: 11px; color: #777; }
`;

	function buildPanel() {
		const style = document.createElement('style');
		style.textContent = PANEL_CSS;
		document.head.appendChild(style);

		const panel = document.createElement('div');
		panel.id = 'kid-panel';
		panel.innerHTML = `
			<div id="kid-body">
				<h3>ILIAS bulk download</h3>
				<label for="kid-scope">Scope</label>
				<select id="kid-scope">
					<option value="current">Current page (recursive)</option>
					<option value="desktop">Dashboard favourites</option>
					<option value="all">All courses</option>
				</select>
				<label for="kid-jobs">Parallel downloads</label>
				<input id="kid-jobs" type="number" min="1" max="8" value="2">
				<label for="kid-rate">Requests per minute</label>
				<input id="kid-rate" type="number" min="1" max="120" value="20">
				<label class="kid-check"><input id="kid-videos" type="checkbox"> Include Opencast videos (uses a lot of memory!)</label>
				<button id="kid-start">Download as ZIP</button>
				<button id="kid-cancel">Cancel</button>
				<div id="kid-status"></div>
				<div id="kid-errors"></div>
				<div id="kid-note">Files are fetched with your current ILIAS session and packed into one ZIP in the browser. Keep this tab in the foreground.</div>
			</div>
			<button id="kid-toggle">ILIAS &#8595;</button>
		`;
		document.body.appendChild(panel);

		const body = panel.querySelector('#kid-body');
		const toggle = panel.querySelector('#kid-toggle');
		const startBtn = panel.querySelector('#kid-start');
		const cancelBtn = panel.querySelector('#kid-cancel');
		const statusEl = panel.querySelector('#kid-status');
		const errorsEl = panel.querySelector('#kid-errors');

		toggle.addEventListener('click', () => body.classList.toggle('kid-open'));

		const ui = {
			currentState: null,
			updateProgress(state, lastFile) {
				const s = state.stats;
				const mb = (s.bytes / 1024 / 1024).toFixed(1);
				statusEl.textContent =
					`Pages crawled: ${s.pages}\n` +
					`Files: ${s.filesDone}/${s.filesTotal} (${mb} MB)` +
					(lastFile ? `\nLast: ${lastFile}` : '');
				if (state.errors.length) {
					errorsEl.textContent = 'Errors (' + state.errors.length + '):\n' + state.errors.slice(-8).join('\n');
				}
			},
			setRunning(state) {
				this.currentState = state;
				startBtn.disabled = true;
				cancelBtn.style.display = 'block';
				statusEl.textContent = 'Starting…';
				errorsEl.textContent = '';
			},
			setDone(state, filename) {
				startBtn.disabled = false;
				cancelBtn.style.display = 'none';
				const s = state.stats;
				let text;
				if (state.aborted) {
					text = 'Cancelled.';
				} else if (filename && s.filesDone > 0) {
					text = `Done: ${s.filesDone} files (${(s.bytes / 1024 / 1024).toFixed(1)} MB)\nSaved as ${filename}`;
				} else if (s.filesDone === 0) {
					text = 'No downloadable files found.';
				} else {
					text = 'Finished with problems, see errors below.';
				}
				if (s.skipped.length) {
					text += `\nSkipped ${s.skipped.length} items (videos/forums/etc.)`;
				}
				statusEl.textContent = text;
				if (state.errors.length) {
					errorsEl.textContent = 'Errors (' + state.errors.length + '):\n' + state.errors.slice(-8).join('\n');
				}
			},
		};

		startBtn.addEventListener('click', () => {
			const options = {
				scope: panel.querySelector('#kid-scope').value,
				jobs: Math.max(1, parseInt(panel.querySelector('#kid-jobs').value, 10) || 2),
				rate: Math.max(1, parseInt(panel.querySelector('#kid-rate').value, 10) || 20),
				includeVideos: panel.querySelector('#kid-videos').checked,
			};
			runDownload(options, ui);
		});

		cancelBtn.addEventListener('click', () => {
			if (ui.currentState) {
				ui.currentState.aborted = true;
				if (ui.currentState.abortController) ui.currentState.abortController.abort();
			}
		});
	}

	// ---------------------------------------------------------------------
	// Test hooks + init
	// ---------------------------------------------------------------------
	const api = {
		fileEscape,
		parseIliasUrl,
		objectFromUrl,
		fileNameFromItem,
		shouldSkipLink,
		getItems,
		contentTabUrl,
		isErrorResponse,
		extensionFromContentType,
		extensionFromContentDisposition,
		nameWithExtension,
		parseOpencastJson,
		collectContentPageLinks,
	};
	if (typeof module !== 'undefined' && module.exports) {
		module.exports = api;
	} else {
		try {
			(typeof unsafeWindow !== 'undefined' ? unsafeWindow : window).KIT_ILIAS_DL = api;
		} catch (e) {
			/* ignore */
		}
	}

	if (typeof document !== 'undefined' && typeof window !== 'undefined' && window.location && window.location.hostname === 'ilias.studium.kit.edu') {
		buildPanel();
	}
})();
