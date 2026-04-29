# Fragment Overrides

Fragments let you replace the default HTML rendering for any YAMD node type with your own Tera template. When bar renders content it checks `[render.fragments]` in `theme.toml` before falling back to built-in rendering.

## How fragments work

For each node type you declare in `theme.toml`, bar:

1. Extracts variables specific to that node type from the parsed content.
2. Renders your `.html` template with those variables as context.
3. Collects your `.css` file's content and injects it into `fragment_styles` on the page.

Fragment templates are standalone Tera renders — they do not inherit from `base.html` and have no access to site-wide context variables.

## Default fragments

The default fragments for all node types live in `src/defaults/fragments/`. Each node type has an `.html` template and a `.css` file. These are compiled into the binary and used when no theme override is provided.

To customize a node type, copy the default files to your theme's fragments directory and modify them.

## Variables per node type

### `image`

| Variable | Type | Description |
|----------|------|-------------|
| `src` | string | Image URL or path |
| `alt` | string | Alt text |
| `lazy_images` | bool | Whether to add `loading="lazy"` (from theme config) |
| `has_services` | bool | Whether template functions are available |

Default template: `src/defaults/fragments/image.html`

Default styles (`src/defaults/fragments/image.css`):

```css
.image { margin: 1em 0; position: relative; }
.image img { max-width: 100%; height: auto; display: block; }
.image picture { display: block; width: 100%; }
.image picture img { width: 100%; display: block; }
.image .fullscreen { position: absolute; bottom: 0.5rem; right: 0.5rem; text-decoration: none; font-size: 1.5rem; opacity: 0.7; }
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
| `text` | string | Plain text of the heading (no markup) |
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
| `has_services` | bool | True when fragment services (e.g. `get_image_url`) are available |

Default template: `src/defaults/fragments/images.html`

Default styles (`src/defaults/fragments/images.css`):

```css
.ig { margin: 1em 0; }
.ig .image { position: relative; overflow-x: auto; scroll-snap-type: x mandatory; display: flex; }
.ig .image > div { scroll-snap-align: start; min-width: 100%; }
.ig .image picture { display: block; width: 100%; }
.ig .image picture img { width: 100%; display: block; }
.ig .image .left, .ig .image .right { position: absolute; top: 50%; transform: translateY(-50%); z-index: 1; font-size: 2rem; text-decoration: none; padding: 0.5rem; }
.ig .image .left { left: 0.5rem; }
.ig .image .right { right: 0.5rem; }
.ig .thumb { display: flex; flex-wrap: wrap; gap: 0.25em; margin-top: 0.5em; }
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

No variables. This node type renders a static element.

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

## CSS requirement

Every fragment declaration **must** include a CSS file. If you have no styles to add, create an empty `.css` file. Bar will fail validation if either the template or the CSS file is missing.

The CSS from all fragment overrides used on a given page is concatenated and exposed as the `fragment_styles` template variable. Your page template is responsible for injecting it (typically in `<head>` via a `<style>` block).

## Example: custom image fragment

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

**`theme.toml`**

```toml
[render.fragments.image]
template = "fragments/image.html"
css = "fragments/image.css"
```

## Example: custom heading fragment

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

When a fragment template fails to render, bar prints:

- The fragment key that failed (e.g. `failed to render fragment template for 'image'`).
- The available variables for that fragment, so you can check for typos.
- The Tera error with a source code pointer if the template has a syntax error.

To see which variables are available at runtime, temporarily add `{{ __tera_context }}` to your fragment template — Tera will dump the full context as JSON.
