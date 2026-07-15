// Smoke test: verify that the Zip + ZipPassThrough + end() pattern used in the
// userscript produces a valid ZIP with the fflate build referenced via @require.
'use strict';

const fflate = require('../vendor/fflate.min.js');
const { execSync } = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

const chunks = [];
let finished = false;

const zip = new fflate.Zip((err, chunk) => {
	if (err) throw err;
	chunks.push(chunk);
});
// same pattern as zipFinish() in the userscript: swap ondata before end()
zip.ondata = (err, chunk, final) => {
	if (err) throw err;
	chunks.push(chunk);
	if (final) finished = true;
};

const e1 = new fflate.ZipPassThrough('Course A/Woche 1/skript.pdf');
zip.add(e1);
e1.push(new TextEncoder().encode('hello pdf'), true);

const e2 = new fflate.ZipPassThrough('Course A/übung (2).txt');
zip.add(e2);
e2.push(new TextEncoder().encode('hello txt'), true);

zip.end();

if (!finished) {
	console.error('FAIL: zip did not signal final chunk');
	process.exit(1);
}

const out = path.join(os.tmpdir(), 'kid-smoke.zip');
fs.writeFileSync(out, Buffer.concat(chunks.map((c) => Buffer.from(c))));
const listing = execSync(`unzip -l ${out}`).toString();
console.log(listing);
if (listing.includes('Course A/Woche 1/skript.pdf') && listing.includes('bung (2).txt')) {
	console.log('ZIP smoke test passed');
} else {
	console.error('FAIL: expected entries not found in zip');
	process.exit(1);
}
