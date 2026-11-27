import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    rolldownOptions: {
      input: ['index.html', 'launchpad/index.html', 'docs/index.html'].map((path) =>
        fileURLToPath(new URL(path, import.meta.url)),
      ),
    },
  },
});
