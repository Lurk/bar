# theme.toml Reference

`theme.toml` lives at the root of the theme directory. Bar validates it on every build and rejects incompatible or malformed configs.

## `[theme]`

| Key | Type | Required | Description |
|-----|------|----------|-------------|
| `name` | string | yes | Theme identifier, used in error messages |
| `version` | string | yes | Theme's own version (semver) |
| `description` | string | yes | Human-readable summary |
| `compatible_bar_versions` | string | yes | Semver requirement for bar itself |
| `tags` | array of strings | yes | Descriptive labels (can be empty) |

### Version compatibility format

`compatible_bar_versions` uses standard semver requirement syntax:

```
">=0.1.0"          # any version at or above 0.1.0
">=0.2.0, <1.0.0"  # range
"^0.3.0"           # compatible with 0.3.x
```

Bar's own version is checked against this requirement at build time. A mismatch is a hard error.

## `[render]`

| Key | Type | Required | Default | Description |
|-----|------|----------|---------|-------------|
| `lazy_images` | bool | yes | — | Add `loading="lazy"` to `<img>` tags in default image rendering |
| `heading_anchors` | bool | yes | — | Wrap heading text in an `<a>` with `href="#anchor_id"` |
| `code_class` | string | no | none | CSS class added to `<pre>` in default code rendering |

## `[render.fragments.NODE_TYPE]`

Overrides the HTML (and CSS) used to render a specific YAMD node type. Both keys are required together — you cannot provide one without the other.

| Key | Type | Description |
|-----|------|-------------|
| `template` | path | Path to the Tera template, relative to the theme directory |
| `css` | path | Path to the CSS file, relative to the theme directory |

`NODE_TYPE` must match a fragment key. Valid keys: `anchor`, `bold`, `code`, `code_span`, `collapsible`, `destination`, `document`, `embed`, `emphasis`, `heading`, `highlight`, `icon`, `image`, `images`, `italic`, `list_item`, `metadata`, `modifier`, `ordered_list`, `paragraph`, `strikethrough`, `thematic_break`, `title`, `unordered_list`.

See [fragments.md](fragments.md) for available template variables per node type.

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

[render.fragments.image]
template = "fragments/image.html"
css = "fragments/image.css"

[render.fragments.code]
template = "fragments/code.html"
css = "fragments/code.css"

[render.fragments.heading]
template = "fragments/heading.html"
css = "fragments/heading.css"
```
