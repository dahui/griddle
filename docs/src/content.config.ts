// Astro 5+ requires content collections to be declared. Starlight ships the loader and schema;
// this file only registers them. Without it every page builds into a site with an empty
// collection and no navigation, which fails as a warning rather than an error.
import { defineCollection } from 'astro:content';
import { docsLoader } from '@astrojs/starlight/loaders';
import { docsSchema } from '@astrojs/starlight/schema';

export const collections = {
  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),
};
