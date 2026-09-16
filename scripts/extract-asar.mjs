import fs from 'node:fs';
import path from 'node:path';

const [input, output] = process.argv.slice(2);
if (!input || !output) throw new Error('Usage: node extract-asar.mjs archive output-directory');
const archive = fs.readFileSync(input);
const headerSize = archive.readUInt32LE(4);
const jsonSize = archive.readUInt32LE(12);
const header = JSON.parse(archive.subarray(16, 16 + jsonSize).toString('utf8'));
const base = 8 + headerSize;
const root = path.resolve(output);
let count = 0;
function walk(files, parent = '') {
  for (const [name, entry] of Object.entries(files)) {
    const relative = path.join(parent, name);
    if (relative.split(path.sep).includes('node_modules')) continue;
    if (entry.files) { walk(entry.files, relative); continue; }
    if (entry.unpacked || entry.link || !/\.(js|json|html|map)$/.test(name)) continue;
    const destination = path.resolve(root, relative);
    if (!destination.startsWith(root + path.sep)) throw new Error('Invalid archive path');
    const start = base + Number(entry.offset);
    const end = start + entry.size;
    if (!Number.isSafeInteger(start) || start < base || end > archive.length) throw new Error('Invalid archive range');
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.writeFileSync(destination, archive.subarray(start, end), { flag: 'wx' });
    count++;
  }
}
walk(header.files);
console.log(`Extracted ${count} application source/configuration files`);
