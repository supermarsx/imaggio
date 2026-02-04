import fs from 'node:fs/promises';
import path from 'node:path';

const root = path.resolve(process.cwd());
const vendorDir = path.join(root, 'app', 'vendor');

async function ensureDir(dir) {
  await fs.mkdir(dir, { recursive: true });
}

async function copyFile(src, dst) {
  await ensureDir(path.dirname(dst));
  await fs.copyFile(src, dst);
}

async function copyDir(srcDir, dstDir) {
  await ensureDir(dstDir);
  const entries = await fs.readdir(srcDir, { withFileTypes: true });
  await Promise.all(
    entries.map(async (entry) => {
      const src = path.join(srcDir, entry.name);
      const dst = path.join(dstDir, entry.name);
      if (entry.isDirectory()) return copyDir(src, dst);
      if (entry.isFile()) return copyFile(src, dst);
    })
  );
}

async function main() {
  const copies = [
    {
      src: path.join(root, 'node_modules', 'jquery', 'dist', 'jquery.min.js'),
      dst: path.join(vendorDir, 'jquery', 'jquery.min.js'),
      kind: 'file'
    },
    {
      src: path.join(root, 'node_modules', 'bulma', 'css', 'bulma.min.css'),
      dst: path.join(vendorDir, 'bulma', 'bulma.min.css'),
      kind: 'file'
    },
    {
      src: path.join(root, 'node_modules', '@fortawesome', 'fontawesome-free', 'css', 'all.min.css'),
      dst: path.join(vendorDir, 'fontawesome', 'css', 'all.min.css'),
      kind: 'file'
    },
    {
      src: path.join(root, 'node_modules', '@fortawesome', 'fontawesome-free', 'webfonts'),
      dst: path.join(vendorDir, 'fontawesome', 'webfonts'),
      kind: 'dir'
    }
  ];

  for (const item of copies) {
    try {
      if (item.kind === 'dir') {
        await copyDir(item.src, item.dst);
      } else {
        await copyFile(item.src, item.dst);
      }
      // eslint-disable-next-line no-console
      console.log(`[vendor] Copied ${path.relative(root, item.src)} -> ${path.relative(root, item.dst)}`);
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn(`[vendor] Skipped ${path.relative(root, item.src)} (${err?.message ?? err})`);
    }
  }
}

await main();
