# Page Templates

Bar use [Tera](https://keats.github.io/tera/) for page templates. All `.html` files in theme dir load as Tera templates, reference each other via Tera `extends`, `include`, `block` directives.

## Template inheritance

Conventional pattern: single `base.html` defines named blocks, page templates extend it:

**`base.html`**

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{% block head %}{{ title }} — {{ config.title }}{% endblock head %}</title>
  <meta name="description" content="{{ description }}">
  <meta property="og:type" content="{% block og_type %}website{% endblock og_type %}">
  <meta property="og:url" content="{% block og_url %}{{ config.domain }}{% endblock og_url %}">
  {% block og_image %}{% endblock og_image %}
  <style>{{ fragment_styles }}</style>
</head>
<body>
  <nav>{% block nav %}{% endblock nav %}</nav>
  <main>{% block content %}{% endblock content %}</main>
</body>
</html>
```

**`article.html`**

```html
{% extends "base.html" %}

{% block og_type %}article{% endblock og_type %}
{% block og_url %}{{ config.domain }}{{ path }}{% endblock og_url %}

{% block content %}
  <article>{{ rendered_body | safe }}</article>
{% endblock content %}
```

Common block names in pattern above: `content`, `nav`, `head`, `og_type`, `og_url`, `og_image`. Conventions, not enforced by bar — name blocks however you like.

## Context variables

Vars available in every page template render:

| Variable | Type | Description |
|----------|------|-------------|
| `config` | object | Site config from `config.yaml` |
| `config.domain` | URL | Site root URL (e.g. `https://example.com/`) |
| `config.title` | string | Site title |
| `config.description` | string | Site description |
| `config.template_config` | map | Arbitrary key/value pairs from `config.yaml` |
| `title` | string | Page title |
| `description` | string | Page description |
| `path` | string | URL path for this page (e.g. `/posts/hello`) |
| `page_num` | integer | Pagination offset (0 for first page) |
| `fragment_styles` | string | Concatenated CSS for all YAMD node types used on this page |
| `rendered_body` | string | Pre-rendered HTML from YAMD content (use `\| safe` to avoid escaping) |

`fragment_styles` and `rendered_body` non-empty only when page path matches content file in `content_path`.

## Custom Tera functions

Functions registered by bar, available in all templates.

### `add_page(path, template, title, description, page_num?)`

Registers dynamic page to render. Bar render loop continues till no unrendered pages remain, so calling this from one template can queue another.

```html
{{ add_page(path="/posts/hello.html", template="article.html", title="Hello", description="My first post") }}
```

| Arg | Default | Description |
|-----|---------|-------------|
| `path` | `/` | URL path for page |
| `template` | `index.html` | Template file to render |
| `title` | `""` | Page title |
| `description` | `""` | Page description |
| `page_num` | `0` | Pagination page number |

### `add_feed(path, type)`

Registers feed to generate. Both args required.

```html
{{ add_feed(path="/feed.json", type="json") }}
{{ add_feed(path="/feed.xml", type="atom") }}
```

Valid `type` values: `json`, `atom`.

### `add_static_file(path, source?)`

Registers static file to copy to dist. Returns `path`.

```html
{% set css = add_static_file(path="/style.css") %}
{% set icon = add_static_file(path="/icon.png", source="/assets/favicon.png") %}
```

`source` omitted → bar copies file at `path` (relative to project root). `source` provided → that file copied to destination `path` in dist.

### `get_static_file(path)`

Returns `path` with cache-busting query param derived from file contents. File must be registered with `add_static_file` first.

```html
<link rel="stylesheet" href="{{ get_static_file(path='/style.css') }}">
{# renders as: /style.css?cb=AbCdEfGh #}
```

### `get_page_by_path(path)`

Returns content page at given URL path, or null if not found. `.html` extension stripped automatically.

```html
{% set page = get_page_by_path(path="/posts/hello.html") %}
```

### `get_page_by_pid(pid)`

Returns content page with given PID (path without extension, relative to `content_path`).

```html
{% set page = get_page_by_pid(pid="/posts/hello") %}
```

### `get_pages_by_tag(tag, limit?, offset?)`

Returns paginated slice of content pages tagged `tag`. Errors if tag not exist.

```html
{% set result = get_pages_by_tag(tag="rust", limit=10, offset=0) %}
{% for page in result.pages %}
  <a href="{{ page.pid }}.html">{{ page.metadata.title }}</a>
{% endfor %}
```

| Arg | Default | Description |
|-----|---------|-------------|
| `tag` | `""` | Tag to filter by |
| `limit` | `3` | Max pages to return |
| `offset` | `0` | Skip this many pages (use `page_num * limit` for pagination) |

Returned object has: `pages` (array), `current_slice`, `total_slices`, `slice_size`, `numbers` (array of `{number, display, is_current}`).

### `get_similar(pid, limit?)`

Returns array of PIDs for pages sharing most tags with given PID.

```html
{% set similar = get_similar(pid=page.pid, limit=3) %}
```

| Arg | Default |
|-----|---------|
| `limit` | `3` |

### `get_image_url(src, width?, height?, ar_width?, ar_height?)`

Returns URL of single resized image variant.

- Local paths (start with `/`): JPEG/PNG/WebP sources resized and re-encoded. `width` only pads image into 16:9 box (default) with blurred-cover backdrop; `width` + `height` crops to that exact box. Output never upscales past source. Variants content-hash named, cached under `.cache/image_variants/`, copied to `dist/<image_output_dir>/` (`image_output_dir` defaults to `images`). Non-raster local files (e.g. SVG) registered as static assets, returned unchanged.
- Cloudinary URLs: applies transformation, returns transformed URL.
- Other URLs: returns URL unchanged.

At least one of `width` or `height` required (no-dimension call on local path falls back to registering file unchanged).

```html
{{ get_image_url(src=page.metadata.image, width=800) }}
{{ get_image_url(src=page.metadata.image, width=400, height=300) }}
{{ get_image_url(src=page.metadata.image, height=600, ar_width=16, ar_height=9) }}
```

### `get_srcset(src, ar_width?, ar_height?)`

Returns complete `srcset` string for `src` across fixed width ladder (`352, 704, 1008, 1568, 2016, 3840`), each entry's width descriptor matching actual emitted variant width. Local sources clamped to intrinsic resolution, duplicate widths deduplicated, so `srcset` never advertises width larger than file. Use together with `get_image_url` (for `src` fallback) and `image_sizes` fragment variable:

```html
<img src="{{ get_image_url(src=src, width=1008) }}"
     srcset="{{ get_srcset(src=src) }}"
     sizes="{{ image_sizes }}"
     alt="{{ alt }}">
```

### `render_gpx(input, width?, height?)`

Renders GPX file to PNG map image, registers as static file, returns image path. Requires GPX embedding configured in `config.yaml`.

```html
<img src="{{ render_gpx(input='/tracks/ride.gpx', width=800, height=400) }}">
```

| Arg | Default |
|-----|---------|
| `width` | `800` |
| `height` | `600` |

### `get_gpx_stats(input)`

Returns distance and elevation stats for GPX file.

```html
{% set stats = get_gpx_stats(input='/tracks/ride.gpx') %}
Distance: {{ stats.distance_km }} km
```

## Custom filters

### `crc32`

Hashes string using SeaHash, returns URL-safe base64 string. Useful for stable identifiers.

```html
{{ page.pid | crc32 }}
```