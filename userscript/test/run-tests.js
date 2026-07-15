// Offline tests for the ILIAS bulk download userscript parsing logic.
// Usage: cd userscript/test && npm install && npm test
'use strict';

const fs = require('fs');
const path = require('path');
const { JSDOM } = require('jsdom');

const script = require('../kit-ilias-downloader.user.js');

let failures = 0;
let passed = 0;

function check(name, actual, expected) {
	const a = JSON.stringify(actual);
	const e = JSON.stringify(expected);
	if (a === e) {
		passed++;
		console.log('  ok: ' + name);
	} else {
		failures++;
		console.error('  FAIL: ' + name + '\n    expected: ' + e + '\n    actual:   ' + a);
	}
}

function loadFixture(name) {
	const html = fs.readFileSync(path.join(__dirname, 'fixtures', name), 'utf8');
	return new JSDOM(html).window.document;
}

// ---------------------------------------------------------------------------
console.log('parseIliasUrl');
{
	const u = script.parseIliasUrl('goto.php/crs/2914319');
	check('goto path course target', u.target, 'crs_2914319');
	check('goto path course ref_id', u.refId, '2914319');

	const f = script.parseIliasUrl('https://ilias.studium.kit.edu/goto.php/file/1234567/download');
	check('goto file download target', f.target, 'file_1234567_download');
	check('goto file download ref_id', f.refId, '1234567');

	const q = script.parseIliasUrl('ilias.php?ref_id=42&cmd=view&baseClass=ilrepositorygui&thr_pk=77');
	check('query baseClass', q.baseClass, 'ilrepositorygui');
	check('query cmd', q.cmd, 'view');
	check('query thr_pk', q.thrPk, '77');
	check('query ref_id', q.refId, '42');
}

// ---------------------------------------------------------------------------
console.log('objectFromUrl');
{
	const course = script.objectFromUrl(script.parseIliasUrl('goto.php/crs/2914319'), 'ProPa', null);
	check('course kind', course.kind, 'course');
	check('course ref_id', course.url.refId, '2914319');

	const grp = script.objectFromUrl(script.parseIliasUrl('goto.php/grp/424242'), 'Gruppe', null);
	check('group is course', grp.kind, 'course');

	const folder = script.objectFromUrl(script.parseIliasUrl('goto.php/fold/2602906'), 'Tutorien', null);
	check('folder kind', folder.kind, 'folder');

	const file = script.objectFromUrl(script.parseIliasUrl('goto.php/file/1234567/download'), 'Skript', null);
	check('file kind', file.kind, 'file');

	// goto.php file link WITHOUT /download suffix must be rewritten
	const fileNoDl = script.objectFromUrl(script.parseIliasUrl('goto.php/file/1234567'), 'Skript', null);
	check('file url rewritten to download', fileNoDl.url.url, 'https://ilias.studium.kit.edu/goto.php/file/1234567/download');

	// content page ?file_id= link must be rewritten
	const fileId = script.objectFromUrl(
		script.parseIliasUrl('ilias.php?baseClass=ilrepositorygui&cmd=sendfile&file_id=987654'),
		'Foliensatz 03',
		null
	);
	check('file_id kind', fileId.kind, 'file');
	check('file_id url rewritten', fileId.url.url, 'https://ilias.studium.kit.edu/goto.php/file/987654/download');

	const thread = script.objectFromUrl(script.parseIliasUrl('ilias.php?thr_pk=99&cmd=viewThread'), 't', null);
	check('thread kind', thread.kind, 'thread');

	const forum = script.objectFromUrl(script.parseIliasUrl('ilias.php?ref_id=1&cmd=showThreads'), 'f', null);
	check('forum kind', forum.kind, 'forum');

	const dashboard = script.objectFromUrl(
		script.parseIliasUrl('ilias.php?baseClass=ilDashboardGUI&cmd=show'),
		'',
		null
	);
	check('dashboard kind', dashboard.kind, 'dashboard');

	const folderView = script.objectFromUrl(
		script.parseIliasUrl('ilias.php?ref_id=1943526&cmd=view&cmdClass=ilobjfoldergui&baseClass=ilrepositorygui'),
		'Übungsblätter',
		null
	);
	check('ilrepositorygui view is folder', folderView.kind, 'folder');

	const exercise = script.objectFromUrl(script.parseIliasUrl('goto.php/exc/1111'), 'Blatt', null);
	check('exercise kind', exercise.kind, 'exercise');

	const wiki = script.objectFromUrl(script.parseIliasUrl('goto.php/wiki/2222'), 'Wiki', null);
	check('wiki kind', wiki.kind, 'wiki');
}

// ---------------------------------------------------------------------------
console.log('shouldSkipLink');
{
	check('mail link skipped', script.shouldSkipLink('ilias.php?baseClass=ilmailgui'), true);
	check('membership jump skipped', script.shouldSkipLink('ilias.php?cmd=jumpToMemberships'), true);
	check('normal link kept', script.shouldSkipLink('goto.php/crs/1'), false);
}

// ---------------------------------------------------------------------------
console.log('getItems: folder listing');
{
	const doc = loadFixture('folder.html');
	const items = script.getItems(doc, 'https://ilias.studium.kit.edu/ilias.php?ref_id=1&cmd=view&baseClass=ilrepositorygui');
	check('item count (mail link skipped)', items.length, 4);
	check('kinds', items.map((i) => i.kind).sort(), ['course', 'file', 'folder', 'folder']);
	const file = items.find((i) => i.kind === 'file');
	check('file name got .pdf from item property', file.name, 'Skript Kapitel 1.pdf');
	check('file download url', file.url.url, 'https://ilias.studium.kit.edu/goto.php/file/1234567/download');
}

// ---------------------------------------------------------------------------
console.log('getItems: dashboard favourites');
{
	const doc = loadFixture('dashboard.html');
	const items = script.getItems(doc, 'https://ilias.studium.kit.edu/ilias.php?baseClass=ilDashboardGUI&cmd=show');
	check('favourite count', items.length, 2);
	check('all courses', items.every((i) => i.kind === 'course'), true);
	check('names', items.map((i) => i.name), ['Programmierparadigmen', 'Communication Systems and Protocols']);
}

// ---------------------------------------------------------------------------
console.log('getItems: content page file links');
{
	const doc = loadFixture('content_page.html');
	const items = script.getItems(doc, 'https://ilias.studium.kit.edu/ilias.php?ref_id=2&cmd=view&baseClass=ilrepositorygui');
	check('one file found (broken link ignored)', items.length, 1);
	check('size suffix stripped from name', items[0].name, 'Foliensatz 03');
	check('rewritten to goto download url', items[0].url.url, 'https://ilias.studium.kit.edu/goto.php/file/987654/download');
}

// ---------------------------------------------------------------------------
console.log('getItems: membership overview (cards)');
{
	const doc = loadFixture('memberships.html');
	const items = script.getItems(doc, 'https://ilias.studium.kit.edu/ilias.php?cmdClass=ilmembershipoverviewgui&baseClass=ilmembershipoverviewgui');
	const courses = items.filter((i) => i.kind === 'course');
	check('two valid courses', courses.length, 2);
	check('course names', courses.map((i) => i.name), ['Programmierparadigmen (WS 25-26)', 'Übungsgruppe 7']);
}

// ---------------------------------------------------------------------------
console.log('contentTabUrl');
{
	const doc = loadFixture('tab_view_content.html');
	check(
		'inactive content tab resolved',
		script.contentTabUrl(doc),
		'https://ilias.studium.kit.edu/ilias.php?ref_id=555&cmd=view&cmdClass=ilobjfoldergui&baseClass=ilrepositorygui'
	);
	const active = new JSDOM('<li id="tab_view_content" class="active"><a href="x">Inhalt</a></li>').window.document;
	check('active content tab returns null', script.contentTabUrl(active), null);
}

// ---------------------------------------------------------------------------
console.log('extension inference');
{
	check('content-type pdf', script.extensionFromContentType('application/pdf; charset=utf-8'), 'pdf');
	check('content-type unknown', script.extensionFromContentType('application/x-unknown'), null);
	check('content-disposition quoted', script.extensionFromContentDisposition('attachment; filename="Blatt 1.pdf"'), 'pdf');
	check(
		'content-disposition rfc5987',
		script.extensionFromContentDisposition("attachment; filename*=UTF-8''%C3%9Cbung%202.pptx"),
		'pptx'
	);
	const headers = new Headers({ 'content-type': 'application/pdf' });
	check('nameWithExtension adds ext', script.nameWithExtension('Skript Kapitel 2', headers), 'Skript Kapitel 2.pdf');
	check('nameWithExtension keeps existing', script.nameWithExtension('a.pdf', headers), 'a.pdf');
}

// ---------------------------------------------------------------------------
console.log('fileEscape');
{
	check('slashes and windows chars replaced', script.fileEscape('a/b\\c:d<e>f"g|h?i*j'), 'a-b-c-d-e-f-g-h-i-j');
}

// ---------------------------------------------------------------------------
console.log('parseOpencastJson');
{
	const streamsHtml =
		'foo {"streams":[{"sources":{"mp4":[{"src":"https://oc.example/video.mp4"}]}}],"x":1}, {"paella_config_file":"y"} bar';
	const json = script.parseOpencastJson(streamsHtml);
	check('opencast streams src', json.streams[0].sources.mp4[0].src, 'https://oc.example/video.mp4');
}

// ---------------------------------------------------------------------------
console.log('isErrorResponse');
{
	const err = new JSDOM('<div class="alert-danger">Fehler</div>').window.document;
	check('alert-danger detected', script.isErrorResponse(err), true);
	const ok = new JSDOM('<div>ok</div>').window.document;
	check('normal page ok', script.isErrorResponse(ok), false);
}

console.log('');
if (failures) {
	console.error(failures + ' test(s) FAILED, ' + passed + ' passed');
	process.exit(1);
} else {
	console.log('All ' + passed + ' tests passed');
}
