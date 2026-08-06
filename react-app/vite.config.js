import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

function logOutputFiles() {
  return {
    name: 'log-output-files',
    closeBundle() {
      const outDir = path.resolve(__dirname, '../public');
      const totals = { raw: 0, gzip: 0 };
      console.log('\n📁 Complete output:');
      walkDir(outDir, '', totals);
      console.log(
        `\n  Total: ${(totals.raw / 1024).toFixed(1)} kB │ gzip: ${(totals.gzip / 1024).toFixed(1)} kB`,
      );
    },
  };
}

function walkDir(dir, prefix, totals) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    const displayPath = prefix + entry.name;
    if (entry.isDirectory()) {
      walkDir(fullPath, displayPath + '/', totals);
    } else {
      const raw = fs.statSync(fullPath).size;
      const gzip = zlib.gzipSync(fs.readFileSync(fullPath), { level: 9 }).length;
      totals.raw += raw;
      totals.gzip += gzip;
      console.log(
        `  ${displayPath.padEnd(40)} ${(raw / 1024).toFixed(1).padStart(7)} kB │ gzip: ${(gzip / 1024).toFixed(1).padStart(7)} kB`,
      );
    }
  }
}

export default defineConfig({
  plugins: [react(), logOutputFiles()],
  build: {
    outDir: '../public',
    emptyOutDir: true,
  },
});
