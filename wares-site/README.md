# Lumen Warehouse

The official package registry UI for Lumen.

## Setup

```bash
npm install
npm run dev
```

## Build

```bash
npm run build
```

Output is in `.output/public`.

## Deploy to Cloudflare Pages

1. Install Wrangler:
```bash
npm install -g wrangler
```

2. Login to Cloudflare:
```bash
wrangler login
```

3. Deploy:
```bash
wrangler pages deploy .output/public
```

Or connect to a GitHub repo for automatic deployments.

## Environment

The site expects these environment variables:
- `API_BASE` - Default: `https://warez.lumen-lang.com/v1`

## Project Structure

```
warez-site/
├── app.vue              # Root component
├── nuxt.config.ts       # Nuxt configuration
├── tailwind.config.ts   # Tailwind theme
├── pages/
│   ├── index.vue        # Homepage
│   ├── search.vue       # Search/browse page
│   └── ware/[name].vue # Package detail page
├── components/
│   ├── AppHeader.vue    # Site header
│   ├── AppFooter.vue    # Site footer
│   ├── SearchBar.vue    # Search input
│   ├── WareCard.vue     # Package card
│   ├── StatsDisplay.vue # Stats grid
│   └── Skeleton.vue     # Loading skeleton
├── composables/
│   └── useWarezApi.ts   # API client
└── assets/css/
    └── main.css         # Global styles
```
