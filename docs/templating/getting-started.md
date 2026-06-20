# Getting Started with Themes

Themes control how bar render site. Theme = directory of Tera HTML templates, optional CSS, and `theme.toml` config file.

## Minimal theme structure

```
my-theme/
├── theme.toml          # required — theme metadata and render config
├── static/             # static assets copied to dist as-is
│   └── style.css
├── index.html          # required — home page template
├── 404.html            # optional — custom 404 page
└── fragments/          # optional — per-node-type HTML overrides
    ├── image.html
    └── image.css
```

Point `config.yaml` at theme directory:

```yaml
template: ./my-theme/
```

## Required files

| File | Purpose |
|------|---------|
| `theme.toml` | Declares theme metadata and render options. Bar refuses to build without it. |
| `index.html` | Rendered for site root (`/`). |

Any `.html` file in theme directory available as Tera template. Bar pre-registers `index.html` and `404.html` automatically; all other pages must be registered by calling `add_page()` from within template.

## Minimal theme.toml

```toml
[theme]
name = "my-theme"
version = "1.0.0"
description = "My personal blog theme"
compatible_bar_versions = ">=0.1.0"
tags = ["blog"]

[render]
lazy_images = false
heading_anchors = true
```

See [theme-config.md](theme-config.md) for full reference.

## Testing your theme

From project root (directory containing `config.yaml`):

```bash
cargo run -- build
```

Built output goes to directory specified by `dist_path` in `config.yaml` (default `./dist`).

Wipe dist folder and tile/alt-text cache before fresh build:

```bash
cargo run -- clear && cargo run -- build
```