import { fileURLToPath } from 'node:url';
import { readFile } from 'node:fs/promises';
import { compile } from 'tailwindcss';
import { defineConfig } from 'vite';

const utilities = 'virtual:launchpad-utilities.css';
const components = ['terminal.jsx', 'market-ui.jsx', 'market-dialogs.jsx'].map((name) =>
  fileURLToPath(new URL('./src/' + name, import.meta.url))
);

export default defineConfig({
  plugins: [
    {
      name: 'launchpad-tailwind',
      resolveId(id) {
        if (id === utilities) return '\0' + utilities;
      },
      async load(id) {
        if (id !== '\0' + utilities) return;
        const candidates = new Set();
        for (const path of components) {
          this.addWatchFile(path);
          const source = await readFile(path, 'utf8');
          for (const token of source.match(/[\w:/%.[\]-]+/g) ?? []) candidates.add(token);
        }
        const compiler = await compile(
          '@theme { --spacing: .25rem; --font-mono: var(--mono); } @tailwind utilities;'
        );
        return compiler.build([...candidates]);
      },
    },
  ],
  build: {
    rolldownOptions: {
      input: [
        'index.html',
        'launchpad/index.html',
        'launchpad/testnet/index.html',
        'docs/index.html',
      ].map((path) => fileURLToPath(new URL(path, import.meta.url))),
    },
  },
});
