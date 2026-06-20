# Fragment Overrides

Fragments let you replace default HTML rendering for any YAMD node type with own Tera template.

## How fragments work

Bar renders each YAMD node via built-in default fragment. Theme overrides fragment by placing `fragments/<key>.html` (and optionally `fragments/<key>.css`) in its template directory. Bar detects files by filename convention at startup.

For each node type, bar:

1. Extracts variables specific to that node type from parsed content.
2. Renders `.html` template (theme override if present, else built-in default) with those variables as context.
3. Collects matching `.css` file content, injects into `fragment_styles` on page.

Fragment templates standalone Tera renders — no inherit from `base.html`, no access to site-wide context variables.

## Default fragments

Default fragments for all node types live in `src/defaults/fragments/`. Each node type has `.html` template and `.css` file. Compiled into binary, used when no theme override present.

To customize node type, place own `fragments/<key>.html` (and optionally `fragments/<key>.css`) in theme directory. Bar uses them automatically.

### Available keys

Every overridable key (use as `fragments/<key>.html`):

`anchor`, `bold`, `code`, `code_span`, `collapsible`, `embed`, `emphasis`, `heading`, `highlight`, `icon`, `image`, `images`, `italic`, `list_item`, `ordered_list`, `paragraph`, `picture`, `strikethrough`, `thematic_break`, `unordered_list`

`picture` is not a YAMD node — shared image renderer included by `image`, `images`, `embed`. See its section below.

## Variables per node type

### `image`

| Variable | Type | Description |
|----------|------|-------------|
| `src` | string | Image URL or path |
| `alt` | string | Alt text |
| `lazy_images` | bool | Whether to add `loading="lazy"` (from theme config) |
| `image_sizes` | string | Value for the `<img sizes>` attribute |
| `has_services` | bool | Whether template functions are available |

Default template: `src/defaults/fragments/image.html`

Default `image` template delegates rendering to shared `picture` fragment (via `{% include %}`), which emits `.image` box and inline fullscreen button. All CSS rules once in `image.css` now live in `picture.css`. `image.css` empty.

### `picture`

`picture` is shared image renderer included by `image`, `images` (per slide), and `embed` (for GPX maps). Emits `.image` wrapper `<div>`, `<img>` element with optional `srcset`/`sizes`/`loading="lazy"`, and inline fullscreen button. Override to change how every image surface looks at once.

| Variable | Type | Description |
|----------|------|-------------|
| `src` | string | Image URL or path. Used directly as the `<img src>` only in the no-services branch; in the services branch the `src` fallback is derived from the smallest `srcset` candidate (a published variant), since the raw `src` may be an unpublished original |
| `srcset` | string | Pre-computed srcset string. Not inserted by bar — the including fragment sets it (e.g. `{% set srcset = get_srcset(src=src) %}`); the no-services branch omits it |
| `alt` | string | Alt text |
| `image_sizes` | string | Value for the `<img sizes>` attribute |
| `lazy_images` | bool | Whether to add `loading="lazy"` to the `<img>` |
| `has_services` | bool | True when service functions (e.g. `get_srcset`) are available |
| `fullscreen` | bool | Controls the inline fullscreen button; defaults to `true` (galleries pass `false` and render one gallery-level button instead) |

Default template: `src/defaults/fragments/picture.html`

Default styles (`src/defaults/fragments/picture.css`):

```css
.image { margin: 1em 0; position: relative; }
.image img { width: 100%; aspect-ratio: 16 / 9; display: block; }
.image .fullscreen { position: absolute; bottom: 0.5rem; right: 0.5rem; cursor: pointer; text-decoration: none; font-size: 1.5rem; opacity: 0.7; color: #fff; text-shadow: 0 0 4px rgba(0, 0, 0, 0.7); }
.image .fullscreen:hover { opacity: 1; }
```

### `code`

| Variable | Type | Description |
|----------|------|-------------|
| `language` | string | Language identifier (e.g. `rust`, `js`) |
| `content` | string | Raw code content |
| `highlighted` | string | Syntax-highlighted HTML produced by syntect |
| `code_class` | string | CSS class for the `<pre>` element (from theme config, default "code") |

Default template: `src/defaults/fragments/code.html`

Default styles (`src/defaults/fragments/code.css`):

```css
pre.code { overflow-x: auto; padding: 1em; background: #2b2a2a; color: #f8f8f2; border-radius: 4px; }
pre.code code { font-family: monospace; }
```

### `heading`

| Variable | Type | Description |
|----------|------|-------------|
| `level` | integer | Heading depth: 1–6 |
| `body` | string | Pre-rendered HTML body (text plus any inline anchors) — pipe through `\| safe` |
| `text` | string | Plain-text concatenation of body text and anchor labels (no markup) |
| `anchor_id` | string | URL-safe slug derived from `text` |
| `heading_anchors` | bool | Whether to add `id` attribute (from theme config) |

Default template: `src/defaults/fragments/heading.html`

Default styles (`src/defaults/fragments/heading.css`):

```css
h1, h2, h3, h4, h5, h6 { margin-top: 1.5em; margin-bottom: 0.5em; }
```

### `anchor`

| Variable | Type | Description |
|----------|------|-------------|
| `href` | string | Link destination |
| `text` | string | Link label text |

Default template: `src/defaults/fragments/anchor.html`

### `embed`

| Variable | Type | Description |
|----------|------|-------------|
| `kind` | string | Embed type identifier (e.g. `youtube`, `gpx`) |
| `args` | string | Embed arguments (e.g. video ID or GPX file path) |
| `has_services` | bool | Whether fragment services (GPX rendering, etc.) are available |
| `icon_elevation` | string | Rendered icon HTML for elevation (GPX only) |
| `icon_distance` | string | Rendered icon HTML for distance (GPX only) |
| `icon_download` | string | Rendered icon HTML for download (GPX only) |

Default template: `src/defaults/fragments/embed.html`

Default styles (`src/defaults/fragments/embed.css`):

```css
.embed { margin: 1em 0; }
.embed iframe { width: 100%; aspect-ratio: 16/9; border: none; }
.gpx-map .image { display: block; width: 100%; }
.gpx-map img { width: 100%; display: block; }
.gpx-embed.with-icon { display: flex; gap: 1rem; padding: 0.5rem 0; align-items: center; }
.gpx-embed.with-icon div { display: flex; align-items: center; gap: 0.25rem; }
```

### `collapsible`

| Variable | Type | Description |
|----------|------|-------------|
| `id` | string | Slugified title, used as the `<input>` id and matching `<label for>`. Falls back to `collapsible-{op_index}` when the title has no alphanumeric characters |
| `title` | string | Summary text shown in the collapsed state |
| `toggle_icon` | string | Rendered icon HTML for the toggle indicator (default: "play" icon) |
| `content` | string | Inner HTML rendered by the default renderer |

Default template: `src/defaults/fragments/collapsible.html`

Default styles (`src/defaults/fragments/collapsible.css`):

```css
.collapsible { margin: 1em 0; }
.collapsible input[type="checkbox"] { display: none; }
.collapsible label { cursor: pointer; font-weight: bold; display: block; }
.collapsible .body { display: none; }
.collapsible input[type="checkbox"]:checked ~ .body { display: block; }
```

### `highlight`

| Variable | Type | Description |
|----------|------|-------------|
| `icon` | string | Rendered icon HTML (empty if no icon) |
| `title` | string | Highlight title text (empty if no title) |
| `content` | string | Inner HTML rendered by the default renderer |

Default template: `src/defaults/fragments/highlight.html`

Default styles (`src/defaults/fragments/highlight.css`):

```css
.highlight { display: grid; gap: 1rem; margin-bottom: 1rem; padding: 1rem; }
.highlight:has(.hicon) { grid-template-columns: 1fr 9fr; }
.highlight .hicon { display: flex; justify-content: center; align-items: center; }
.highlight .body { display: flex; flex-direction: column; justify-content: center; }
.highlight .htitle { font-weight: bold; }
```

### `images`

| Variable | Type | Description |
|----------|------|-------------|
| `content` | string | Inner HTML rendered by the default renderer |
| `images` | array | List of `{src, alt}` objects for each image in the gallery |
| `image_sizes` | string | Value for the `<img sizes>` attribute |
| `lazy_images` | bool | Whether to add `loading="lazy"` to slide images (forwarded to `picture` per slide) |
| `has_services` | bool | True when fragment services (e.g. `get_image_url`) are available |

Default template: `src/defaults/fragments/images.html`

Default styles (`src/defaults/fragments/images.css`):

```css
.ig { margin: 1em 0; }
.ig > .frame { position: relative; }
.ig > .frame > .image > div { display: none; position: relative; }
.ig > .frame > .image > div:target { display: block; }
.ig > .frame > .image:not(:has(> div:target)) > div:first-child { display: block; }
.ig > .frame > .image > div > .image { margin: 0; }
.ig > .frame > .image > div > .image > img { width: 100%; aspect-ratio: 16 / 9; display: block; }
.ig > .frame > .image .left, .ig > .frame > .image .right { position: absolute; top: 50%; transform: translateY(-50%); z-index: 1; font-size: 2rem; text-decoration: none; padding: 0.5rem; color: #fff; text-shadow: 0 0 4px rgba(0, 0, 0, 0.7); }
.ig > .frame > .image .left { left: 0.5rem; }
.ig > .frame > .image .right { right: 0.5rem; }
.ig > .frame > .fullscreen { position: absolute; bottom: 0.5rem; right: 0.5rem; cursor: pointer; text-decoration: none; font-size: 1.5rem; opacity: 0.7; z-index: 1; color: #fff; text-shadow: 0 0 4px rgba(0, 0, 0, 0.7); }
.ig > .frame > .fullscreen:hover { opacity: 1; }
.ig > .frame:fullscreen { background: #000; display: flex; align-items: center; justify-content: center; }
.ig > .frame:fullscreen > .image,
.ig > .frame:fullscreen > .image > div:target,
.ig > .frame:fullscreen > .image:not(:has(> div:target)) > div:first-child,
.ig > .frame:fullscreen > .image > div > .image { display: contents; }
.ig > .frame:fullscreen img { max-width: 100%; max-height: 100%; width: auto; height: auto; aspect-ratio: auto; object-fit: contain; }
.ig .thumb { display: flex; flex-wrap: wrap; justify-content: center; gap: 0.25em; margin-top: 0.5em; }
.ig .thumb img { display: block; }
```

### `paragraph`

| Variable | Type | Description |
|----------|------|-------------|
| `content` | string | Inner HTML rendered by the default renderer |

Default template: `src/defaults/fragments/paragraph.html`

### `unordered_list`

| Variable | Type | Description |
|----------|------|-------------|
| `content` | string | Inner HTML rendered by the default renderer |

Default template: `src/defaults/fragments/unordered_list.html`

### `ordered_list`

| Variable | Type | Description |
|----------|------|-------------|
| `content` | string | Inner HTML rendered by the default renderer |

Default template: `src/defaults/fragments/ordered_list.html`

### `list_item`

| Variable | Type | Description |
|----------|------|-------------|
| `content` | string | Inner HTML rendered by the default renderer |

Default template: `src/defaults/fragments/list_item.html`

### `thematic_break`

No variables. Renders static element.

Default template: `src/defaults/fragments/thematic_break.html`

Default styles (`src/defaults/fragments/thematic_break.css`):

```css
hr { border: none; border-top: 1px solid currentColor; margin: 2em 0; opacity: 0.3; }
```

### `icon`

| Variable | Type | Description |
|----------|------|-------------|
| `name` | string | Icon identifier (e.g. `github`, `calendar`) |

Default template: `src/defaults/fragments/icon.html`

Default styles (`src/defaults/fragments/icon.css`):

```css
.icon { display: inline-block; }
```

### Inline text fragments

`bold`, `italic`, `emphasis`, `strikethrough`, `code_span` are inline nodes. Each takes one variable:

| Variable | Type | Description |
|----------|------|-------------|
| `content` | string | Inner HTML rendered by the default renderer — pipe through `\| safe` |

Defaults (all have empty `.css`):

| Key | YAMD | Default template | Output |
|-----|------|------------------|--------|
| `bold` | `**x**` | `src/defaults/fragments/bold.html` | `<b>` |
| `italic` | `_x_` | `src/defaults/fragments/italic.html` | `<i>` |
| `emphasis` | `*x*` | `src/defaults/fragments/emphasis.html` | `<em>` |
| `strikethrough` | `~~x~~` | `src/defaults/fragments/strikethrough.html` | `<s>` |
| `code_span` | `` `x` `` | `src/defaults/fragments/code_span.html` | `<code>` |

## CSS convention

When theme provides `fragments/<key>.html`, bar also checks for `fragments/<key>.css`. If CSS file exists, used instead of built-in default; if absent, built-in CSS kept. To suppress all default styles, supply empty `.css` file.

CSS from all fragment overrides used on a page concatenated, exposed as `fragment_styles` template variable. Your page template responsible for injecting it (typically in `<head>` via `<style>` block).

## Example: custom image fragment

Place two files in theme's `fragments/` directory — no `theme.toml` entry required:

**`fragments/image.html`**

```html
<figure>
  <img src="{{ src }}" alt="{{ alt }}"{% if lazy_images %} loading="lazy"{% endif %}/>
  <figcaption>{{ alt }}</figcaption>
</figure>
```

**`fragments/image.css`**

```css
.image {
  margin: 1.5em 0;
}
.image img {
  max-width: 100%;
  height: auto;
  display: block;
  border-radius: 4px;
}
.image figcaption {
  font-size: 0.85em;
  color: #666;
  margin-top: 0.4em;
}
```

## Example: custom heading fragment

Place `fragments/heading.html` and `fragments/heading.css` in theme directory:

**`fragments/heading.css`**

```css
h1, h2, h3, h4, h5, h6 {
  position: relative;
}
.heading-anchor {
  margin-right: 0.4em;
  opacity: 0.3;
  text-decoration: none;
}
.heading-anchor:hover {
  opacity: 1;
}
```

## Debugging

When fragment template fails to render, bar prints:

- Failed fragment key (e.g. `failed to render fragment template for 'image'`).
- Available variables for that fragment, so you can check typos.
- Tera error with source code pointer if template has syntax error.

To see which variables available at runtime, temporarily add `{{ __tera_context }}` to fragment template — Tera dumps full context as JSON.