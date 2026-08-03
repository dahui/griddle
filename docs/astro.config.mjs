// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLinksValidator from 'starlight-links-validator';

// `site` and `base` are both required, and omitting `base` is the standard GitHub Pages footgun:
// the site builds, deploys, and then serves a page whose every stylesheet and image 404s, because
// a project page lives under /<repo>/ rather than at the domain root. Nothing warns about it.
const SITE = 'https://dahui.github.io';
const BASE = '/griddle';

export default defineConfig({
  site: SITE,
  base: BASE,
  // Griddle is a Windows app, and these pages get linked from release notes and shared as URLs.
  // Trailing slashes on every link keeps those stable rather than depending on how the host
  // happens to redirect.
  trailingSlash: 'always',
  integrations: [
    starlight({
      title: 'Griddle',
      description:
        'Artwork for your Steam library, applied instantly. A Windows app for browsing and ' +
        'applying SteamGridDB artwork without restarting Steam.',
      // Served from `public/`, so the path is site-absolute and must NOT include `base` -- Astro
      // prefixes it. Writing '/griddle/favicon.png' here yields '/griddle/griddle/favicon.png'.
      favicon: '/favicon.png',
      // The artwork is itself a wordmark and reads at nav height, so the text title would print
      // the name twice. Starlight still emits an `sr-only` title alongside, so turning the visible
      // one off costs nothing for a screen reader. The previous artwork stacked its word under a
      // badge and could not do this -- at Starlight's ~40px cap the word was a smudge.
      logo: { src: './src/assets/logo.png', replacesTitle: true },
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/dahui/griddle' },
      ],
      editLink: {
        baseUrl: 'https://github.com/dahui/griddle/edit/main/docs/',
      },
      lastUpdated: true,
      customCss: ['./src/styles/custom.css'],
      // A broken internal link fails the build rather than shipping. These pages cross-reference
      // heavily -- the install page hands off to the API key page, which hands off to the
      // walkthrough -- and a renamed slug would otherwise 404 silently on a deployed site.
      plugins: [starlightLinksValidator({ errorOnRelativeLinks: false })],
      // Ordered by the journey a new user actually takes, not by topic. The first four pages are
      // the uninterrupted path from "found this" to "changed a capsule"; nothing else competes
      // for that space. Administrative and internals material sits below it, deliberately out of
      // the way of someone who only wants to use the app.
      sidebar: [
        {
          label: 'Start here',
          items: [
            { label: 'What Griddle is', slug: 'index' },
            { label: 'Install', slug: 'start/install' },
            { label: 'Your API key', slug: 'start/your-api-key' },
            { label: 'Your first artwork', slug: 'start/first-artwork' },
          ],
        },
        {
          label: 'Using Griddle',
          items: [
            { label: 'Finding your games', slug: 'using/finding-your-games' },
            { label: 'The five artwork types', slug: 'using/artwork-types' },
            { label: 'Filters', slug: 'using/filters' },
            { label: 'Controller and keyboard', slug: 'using/controller-and-keyboard' },
            { label: 'Resetting artwork', slug: 'using/resetting-artwork' },
          ],
        },
        {
          label: 'Good to know',
          items: [
            { label: 'Non-Steam shortcuts', slug: 'notes/non-steam-shortcuts' },
            { label: 'What Griddle changes', slug: 'notes/what-griddle-changes' },
            { label: 'Troubleshooting', slug: 'notes/troubleshooting' },
            { label: 'Uninstalling', slug: 'notes/uninstalling' },
          ],
        },
        {
          label: 'Under the hood',
          collapsed: true,
          items: [
            { label: 'How live apply works', slug: 'internals/how-live-apply-works' },
            { label: 'The write boundary', slug: 'internals/the-write-boundary' },
            { label: 'Testing', slug: 'internals/testing' },
          ],
        },
        {
          label: 'Contributing',
          collapsed: true,
          items: [{ label: 'Building and contributing', slug: 'contributing' }],
        },
      ],
    }),
  ],
});
