import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [
    // Tailwind vóór SvelteKit, zodat de CSS al verwerkt is als Svelte hem ziet.
    tailwindcss(),
    sveltekit({
      compilerOptions: {
        // Runes-modus voor het hele project, behalve bibliotheken.
        runes: ({ filename }) =>
          filename.split(/[/\\]/).includes('node_modules') ? undefined : true
      },
      // Statische SPA voor Tauri: geen SSR, geen prerender, alles via index.html.
      adapter: adapter({ fallback: 'index.html' })
    })
  ],
  // Tauri verwacht een vaste poort en laat Vite het scherm niet wissen.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
    // CHANGELOG.md staat in de root van de repo, een map boven de app.
    fs: { allow: ['.', '../CHANGELOG.md'] }
  },
  test: {
    expect: { requireAssertions: true },
    environment: 'node',
    include: ['src/**/*.{test,spec}.{js,ts}']
  }
});
