# theme.toml Reference

`theme.toml` live at root of theme directory. Bar validate on every build, reject incompatible or malformed config.

## `[theme]`

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `name` | string | yes | Theme identifier, used in error messages |
| `version` | string | yes | Theme's own version (semver) |
| `description` | string | yes | Human-readable summary |
| `compatible_bar_versions` | string | yes | Semver requirement for bar itself |
| `tags` | array of strings | yes | Descriptive labels (can be empty) |

### Version compatibility format

`compatible_bar_versions` use standard semver requirement syntax:

```
">=0.1.0"          # any version at or above 0.1.0
">=0.2.0, <1.0.0"  # range
"^0.3.0"           # compatible with 0.3.x
```

Bar's own version checked against this requirement at build time. Mismatch = hard error.

## `[render]`

| Key | Type | Required | Default | Description |
|-----|------|----------|---------|-------------|
| `lazy_images` | bool | yes | — | Add `loading="lazy"` to `<img>` tags in default image rendering |
| `heading_anchors` | bool | yes | — | Wrap heading text in `<a>` with `href="#anchor_id"` |
| `code_class` | string | no | none | CSS class added to `<pre>` in default code rendering |

## Fragment overrides (filename convention)

To override fragment, place `fragments/<key>.html` (and optionally `fragments/<key>.css`) in theme directory. No TOML declaration needed — bar detect overrides by filename at startup.

Valid keys: `anchor`, `bold`, `code`, `code_span`, `collapsible`, `destination`, `document`, `embed`, `emphasis`, `heading`, `highlight`, `icon`, `image`, `images`, `italic`, `list_item`, `metadata`, `modifier`, `ordered_list`, `paragraph`, `picture`, `strikethrough`, `thematic_break`, `title`, `unordered_list`.

See [fragments.md](fragments.md) for available template variables per node type.

## `[render.image]`

Control responsive image ladder used when services available (local image resizing or Cloudinary). Both keys optional.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `sizes` | string | `(display-mode: fullscreen) 100vw, (min-width: 1008px) 1008px, 100vw` | Value for `<img sizes>` attribute, passed to `picture` as `image_sizes` |
| `widths` | array of integers | `[352, 704, 1008, 1568, 2016, 3840]` | Pixel widths for srcset candidate ladder |

## Full example

```toml
[theme]
name = "my-theme"
version = "2.1.0"
description = "Clean blog theme with custom image and code rendering"
compatible_bar_versions = ">=0.1.0, <2.0.0"
tags = ["blog", "minimal", "dark"]

[render]
lazy_images = true
heading_anchors = true
code_class = "highlight"

[render.image]
sizes = "(min-width: 1008px) 1008px, 100vw"
widths = [352, 704, 1008, 1568, 2016, 3840]
```

Fragment overrides (`fragments/image.html`, `fragments/code.html`, etc.) picked up automatically from `fragments/` directory — no TOML entries needed.