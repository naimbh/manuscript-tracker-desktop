import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, URL } from 'node:url';
import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import tailwindcss from '@tailwindcss/vite';

// pdf.js data files (CMaps, standard fonts, wasm decoders, ICC profiles), served from the app itself at `pdfjs/...`
// so PDF previews work offline. The JS scripting sandbox (quickjs) is left out: PDF scripts never run.
function pdfjsAssets() {
  const base = fileURLToPath(new URL('./node_modules/pdfjs-dist/', import.meta.url));
  const dirs = ['cmaps', 'standard_fonts', 'wasm', 'iccs'];
  const keep = (f) => !f.startsWith('quickjs');
  const files = () => dirs.flatMap((d) => fs.readdirSync(path.join(base, d)).filter(keep).map((f) => `${d}/${f}`));
  return {
    name: 'pdfjs-assets',
    configureServer(server) {
      server.middlewares.use('/pdfjs/', (req, res, next) => {
        const rel = decodeURIComponent((req.url || '').split('?')[0]).replace(/^\/+/, '');
        if (!files().includes(rel)) return next();
        res.setHeader('Content-Type', rel.endsWith('.js') ? 'text/javascript' : 'application/octet-stream');
        fs.createReadStream(path.join(base, rel)).pipe(res);
      });
    },
    generateBundle() {
      for (const rel of files()) this.emitFile({ type: 'asset', fileName: `pdfjs/${rel}`, source: fs.readFileSync(path.join(base, rel)) });
    },
  };
}

export default defineConfig({
  root: fileURLToPath(new URL('./src', import.meta.url)),
  base: './',
  publicDir: false,
  clearScreen: false,
  envPrefix: ['VITE_', 'TAURI_'],
  plugins: [
    vue({
      template: {
        compilerOptions: {
          // Flux renders custom elements (ui-field, ui-menu, ...); they are styled, not registered.
          isCustomElement: (tag) => tag.startsWith('ui-'),
        },
      },
    }),
    tailwindcss(),
    pdfjsAssets(),
  ],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  server: { port: 1420, strictPort: true },
  build: {
    outDir: fileURLToPath(new URL('./dist', import.meta.url)),
    emptyOutDir: true,
    target: ['es2021', 'safari15'],
    assetsInlineLimit: 0,
    chunkSizeWarningLimit: 300,
    rollupOptions: {
      // The icon set loads in parallel with the app code as its own (long-cached) chunk.
      output: { manualChunks: (id) => (id.endsWith('/src/flux/icons.js') ? 'icons' : undefined) },
    },
  },
});
