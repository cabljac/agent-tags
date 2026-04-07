// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import cloudflare from '@astrojs/cloudflare';
import react from '@astrojs/react';
import tailwindcss from '@tailwindcss/vite';

// https://astro.build/config
export default defineConfig({
  site: 'https://agenttags.dev',
  output: 'static',
  adapter: cloudflare(),
  integrations: [
    starlight({
      title: 'agent-tags',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/cabljac/agent-tags' },
      ],
      sidebar: [
        { label: 'Getting Started', slug: 'docs/getting-started' },
        { label: 'Specification', slug: 'docs/spec' },
        { label: 'CLI Reference', slug: 'docs/cli' },
        { label: 'Why agent-tags?', slug: 'docs/why' },
      ],
      customCss: ['./src/styles/custom.css'],
      components: {
        ThemeProvider: './src/components/overrides/ThemeProvider.astro',
        ThemeSelect: './src/components/overrides/ThemeSelect.astro',
      },
      expressiveCode: {
        themes: ['github-dark'],
      },
    }),
    react(),
  ],
  vite: {
    plugins: [tailwindcss()],
  },
});
